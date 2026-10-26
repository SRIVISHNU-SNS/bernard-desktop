use crate::app_state::AppState;
use crate::models::Theme;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(avatar) = app.get_webview_window("avatar") {
        let state = app.state::<AppState>();
        let settings = state.settings();
        let size = settings.avatar_size as f64;
        avatar.set_size(tauri::LogicalSize::new(size, size))?;
        if let (Some(x), Some(y)) = (settings.avatar_x, settings.avatar_y) {
            avatar.set_position(PhysicalPosition::new(x, y))?;
        } else if let Some(monitor) = app.primary_monitor()? {
            let area = monitor.work_area();
            let x = area.position.x + area.size.width as i32 - size as i32 - 24;
            let y = area.position.y + area.size.height as i32 - size as i32 - 24;
            avatar.set_position(PhysicalPosition::new(x, y))?;
        }
        server_capture_exclusion(&avatar);
        avatar.show()?;
        let avatar_handle = app.clone();
        avatar.on_window_event(move |event| {
            if let tauri::WindowEvent::Moved(position) = event {
                note_avatar_move(&avatar_handle, position.x, position.y);
            }
        });
    }

    if let Some(panel) = app.get_webview_window("panel") {
        server_capture_exclusion(&panel);
        panel.hide()?;
        let app_handle = app.clone();
        panel.on_window_event(move |event| {
            if let tauri::WindowEvent::ScaleFactorChanged { .. } = event {
                let _ = app_handle.emit("panel://scale-changed", ());
            }
        });
    }

    create_menu(app)?;
    prewarm_overlays(app)?;
    setup_tray(app)?;
    apply_theme(app);
    Ok(())
}

fn server_capture_exclusion(window: &WebviewWindow) {
    #[cfg(windows)]
    crate::server::set_capture_exclusion(window);
    let _ = window;
}

fn create_menu(app: &AppHandle) -> Result<(), tauri::Error> {
    if app.get_webview_window("menu").is_some() {
        return Ok(());
    }
    let menu = WebviewWindowBuilder::new(
        app,
        "menu",
        WebviewUrl::App("index.html?window=menu".into()),
    )
    .title("Bernard menu")
    .inner_size(224.0, 292.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .visible(false)
    .focused(false)
    .build()?;
    server_capture_exclusion(&menu);
    let handle = app.clone();
    menu.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            if let Some(menu) = handle.get_webview_window("menu") {
                let _ = menu.hide();
            }
            if let Some(state) = handle.try_state::<AppState>() {
                state.menu_visible.store(false, Ordering::SeqCst);
            }
        }
    });
    Ok(())
}

fn prewarm_overlays(app: &AppHandle) -> Result<(), tauri::Error> {
    let count = app.available_monitors()?.len().max(1);
    for index in 0..count {
        let label = format!("overlay-{index}");
        if app.get_webview_window(&label).is_some() {
            continue;
        }
        let window = WebviewWindowBuilder::new(
            app,
            &label,
            WebviewUrl::App(format!("index.html?window=overlay&label={label}").into()),
        )
        .title("Bernard snip")
        .inner_size(320.0, 200.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .focused(false)
        .build()?;
        server_capture_exclusion(&window);
        window.hide()?;
    }
    Ok(())
}

pub fn ensure_overlay_count(app: &AppHandle, count: usize) -> Result<(), String> {
    let current = app
        .webview_windows()
        .keys()
        .filter(|label| label.starts_with("overlay-"))
        .count();
    for index in current..count {
        let label = format!("overlay-{index}");
        let window = WebviewWindowBuilder::new(
            app,
            &label,
            WebviewUrl::App(format!("index.html?window=overlay&label={label}").into()),
        )
        .title("Bernard snip")
        .inner_size(320.0, 200.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| format!("Could not create a snip overlay: {e}"))?;
        server_capture_exclusion(&window);
    }
    Ok(())
}

pub fn show_overlay(
    app: &AppHandle,
    label: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    focus: bool,
) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| "A snip overlay is missing.".to_string())?;
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("Could not position the snip overlay: {e}"))?;
    window
        .set_size(PhysicalSize::new(width.max(64), height.max(64)))
        .map_err(|e| format!("Could not size the snip overlay: {e}"))?;
    window
        .show()
        .map_err(|e| format!("Could not show the snip overlay: {e}"))?;
    if focus {
        window
            .set_focus()
            .map_err(|e| format!("Could not focus the snip overlay: {e}"))?;
    }
    Ok(())
}

pub fn hide_overlays(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with("overlay-") {
            let _ = window.hide();
        }
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.overlay_active.store(false, Ordering::SeqCst);
    }
}

pub fn panel_visible(app: &AppHandle) -> bool {
    app.get_webview_window("panel")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

pub fn anchor_panel(app: &AppHandle) -> Result<(), String> {
    let Some(panel) = app.get_webview_window("panel") else {
        return Err("The chat panel is not available.".to_string());
    };
    let Some(avatar) = app.get_webview_window("avatar") else {
        return Err("The avatar is not available.".to_string());
    };
    let avatar_position = avatar
        .outer_position()
        .map_err(|e| format!("Could not read the avatar position: {e}"))?;
    let avatar_size = avatar
        .outer_size()
        .map_err(|e| format!("Could not read the avatar size: {e}"))?;
    let mut panel_size = panel
        .outer_size()
        .map_err(|e| format!("Could not read the panel size: {e}"))?;
    if panel_size.width == 0 || panel_size.height == 0 {
        panel_size = PhysicalSize::new(420, 600);
    }

    let monitor = app
        .monitor_from_point(
            (avatar_position.x + avatar_size.width as i32 / 2) as f64,
            (avatar_position.y + avatar_size.height as i32 / 2) as f64,
        )
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let (left, top, right, bottom) = if let Some(monitor) = monitor {
        let area = monitor.work_area();
        (
            area.position.x,
            area.position.y,
            area.position.x + area.size.width as i32,
            area.position.y + area.size.height as i32,
        )
    } else {
        (0, 0, 1920, 1080)
    };

    let gap = 10;
    let right_x = avatar_position.x + avatar_size.width as i32 + gap;
    let left_x = avatar_position.x - panel_size.width as i32 - gap;
    let mut x = if right_x + panel_size.width as i32 <= right {
        right_x
    } else if left_x >= left {
        left_x
    } else {
        (right - panel_size.width as i32).max(left)
    };
    let mut y = avatar_position.y + avatar_size.height as i32 - panel_size.height as i32;
    if y + panel_size.height as i32 > bottom {
        y = bottom - panel_size.height as i32;
    }
    y = y.max(top);
    x = x.clamp(left, (right - panel_size.width as i32).max(left));

    panel
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("Could not position the chat panel: {e}"))?;
    Ok(())
}

pub fn show_panel(app: &AppHandle) -> Result<(), String> {
    let Some(panel) = app.get_webview_window("panel") else {
        return Err("The chat panel is not available.".to_string());
    };
    anchor_panel(app)?;
    panel
        .show()
        .map_err(|e| format!("Could not show the chat panel: {e}"))?;
    panel
        .set_focus()
        .map_err(|e| format!("Could not focus the chat panel: {e}"))?;
    if let Some(state) = app.try_state::<AppState>() {
        state.panel_visible.store(true, Ordering::SeqCst);
    }
    let _ = app.emit("panel://opened", ());
    let _ = app.emit("avatar://state", "listening");
    Ok(())
}

pub fn hide_panel(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.hide();
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.panel_visible.store(false, Ordering::SeqCst);
        if !state.overlay_active.load(Ordering::SeqCst) {
            let _ = app.emit("avatar://state", "idle");
        }
    }
    let _ = app.emit("panel://closed", ());
}

pub fn toggle_panel(app: &AppHandle) {
    if panel_visible(app) {
        hide_panel(app);
    } else {
        if let Err(error) = show_panel(app) {
            tracing::warn!("{error}");
        }
    }
}

pub fn show_avatar(app: &AppHandle) {
    if let Some(avatar) = app.get_webview_window("avatar") {
        let _ = avatar.show();
        let _ = app.emit("avatar://state", "idle");
    }
}

pub fn hide_avatar(app: &AppHandle) {
    hide_panel(app);
    if let Some(avatar) = app.get_webview_window("avatar") {
        let _ = avatar.hide();
    }
}

pub fn toggle_menu(app: &AppHandle) -> Result<(), String> {
    let Some(menu) = app.get_webview_window("menu") else {
        return Err("The avatar menu is not available.".to_string());
    };
    let visible = menu.is_visible().unwrap_or(false);
    if visible {
        menu.hide().map_err(|e| format!("Could not hide the menu: {e}"))?;
        return Ok(());
    }
    let Some(avatar) = app.get_webview_window("avatar") else {
        return Ok(());
    };
    let position = avatar
        .outer_position()
        .map_err(|e| format!("Could not read the avatar position: {e}"))?;
    let size = avatar
        .outer_size()
        .map_err(|e| format!("Could not read the avatar size: {e}"))?;
    let menu_size = menu
        .outer_size()
        .map_err(|e| format!("Could not read the menu size: {e}"))?;
    let monitor = app
        .monitor_from_point(
            (position.x + size.width as i32 / 2) as f64,
            (position.y + size.height as i32 / 2) as f64,
        )
        .ok()
        .flatten();
    let (left, top, right, bottom) = if let Some(monitor) = monitor {
        let area = monitor.work_area();
        (
            area.position.x,
            area.position.y,
            area.position.x + area.size.width as i32,
            area.position.y + area.size.height as i32,
        )
    } else {
        (0, 0, 1920, 1080)
    };
    let mut x = position.x + size.width as i32 + 8;
    if x + menu_size.width as i32 > right {
        x = position.x - menu_size.width as i32 - 8;
    }
    let mut y = position.y;
    if y + menu_size.height as i32 > bottom {
        y = bottom - menu_size.height as i32;
    }
    x = x.clamp(left, (right - menu_size.width as i32).max(left));
    y = y.max(top);
    menu.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("Could not position the menu: {e}"))?;
    menu.show().map_err(|e| format!("Could not show the menu: {e}"))?;
    menu.set_focus()
        .map_err(|e| format!("Could not focus the menu: {e}"))?;
    if let Some(state) = app.try_state::<AppState>() {
        state.menu_visible.store(true, Ordering::SeqCst);
    }
    Ok(())
}

pub fn hide_menu(app: &AppHandle) {
    if let Some(menu) = app.get_webview_window("menu") {
        let _ = menu.hide();
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.menu_visible.store(false, Ordering::SeqCst);
    }
}

pub fn apply_avatar_size(app: &AppHandle, size: u32) {
    if let Some(avatar) = app.get_webview_window("avatar") {
        let _ = avatar.set_size(tauri::LogicalSize::new(size as f64, size as f64));
        let _ = anchor_panel(app);
    }
}

pub fn apply_theme(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let settings = state.settings();
    let theme = match settings.theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    };
    for (_, window) in app.webview_windows() {
        let _ = window.set_theme(theme);
    }
    let _ = app.emit("settings://changed", &settings);
}

pub fn note_avatar_move(app: &AppHandle, x: i32, y: i32) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    state.set_avatar_position(x, y);
    let generation = state.drag_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(550)).await;
        let Some(state) = handle.try_state::<AppState>() else {
            return;
        };
        if state.drag_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        snap_and_save(&handle);
    });
}

fn snap_and_save(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Some(avatar) = app.get_webview_window("avatar") else {
        return;
    };
    let Ok(position) = avatar.outer_position() else {
        return;
    };
    let Ok(size) = avatar.outer_size() else {
        return;
    };
    let monitor = app
        .monitor_from_point(
            (position.x + size.width as i32 / 2) as f64,
            (position.y + size.height as i32 / 2) as f64,
        )
        .ok()
        .flatten();
    let Some(monitor) = monitor else {
        persist_state(app);
        return;
    };
    let area = monitor.work_area();
    let left = area.position.x;
    let top = area.position.y;
    let right = area.position.x + area.size.width as i32 - size.width as i32;
    let bottom = area.position.y + area.size.height as i32 - size.height as i32;
    let threshold = 18;
    let mut x = position.x.clamp(left, right.max(left));
    let mut y = position.y.clamp(top, bottom.max(top));
    if (x - left).abs() < threshold {
        x = left;
    } else if (right - x).abs() < threshold {
        x = right;
    }
    if (y - top).abs() < threshold {
        y = top;
    } else if (bottom - y).abs() < threshold {
        y = bottom;
    }
    if x != position.x || y != position.y {
        let _ = avatar.set_position(PhysicalPosition::new(x, y));
    }
    state.set_avatar_position(x, y);
    persist_state(app);
    let _ = anchor_panel(app);
}

pub fn persist_state(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let settings = state.settings();
    let _ = state.db.save_settings(&settings);
}

pub fn apply_autostart(app: &AppHandle) -> Result<(), String> {
    let Some(state) = app.try_state::<AppState>() else {
        return Ok(());
    };
    let enabled = state.settings().start_with_windows;
    let autolaunch = app.autolaunch();
    let current = autolaunch.is_enabled().unwrap_or(false);
    if enabled && !current {
        autolaunch
            .enable()
            .map_err(|e| format!("Could not enable start with Windows: {e}"))?;
    } else if !enabled && current {
        autolaunch
            .disable()
            .map_err(|e| format!("Could not disable start with Windows: {e}"))?;
    }
    Ok(())
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let show = MenuItem::with_id(app, "show", "Show Bernard", true, None::<&str>)?;
    let snip = MenuItem::with_id(app, "snip", "Snip screen", true, Some("Alt+B"))?;
    let hide = MenuItem::with_id(app, "hide", "Hide avatar", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &snip, &hide, &quit])?;
    let mut builder = TrayIconBuilder::with_id("bernard-tray")
        .tooltip("Bernard")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_avatar(app),
            "snip" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    crate::commands::begin_snip(&handle).await;
                });
            }
            "hide" => hide_avatar(app),
            "quit" => {
                persist_state(app);
                crate::server::shutdown(app);
                app.exit(0);
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn set_avatar_badge(app: &AppHandle, state_name: &str) {
    let _ = app.emit("avatar://state", state_name);
}
