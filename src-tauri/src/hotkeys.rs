use crate::app_state::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{
    GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState,
};

pub fn register(app: &AppHandle) -> Result<(), String> {
    let hotkey = app
        .try_state::<AppState>()
        .map(|state| state.settings().hotkey)
        .unwrap_or_else(|| "Alt+B".to_string());
    
    // Register the main snip hotkey
    register_hotkey(app, &hotkey)?;
    
    // Register the 'A' key for toggling visibility
    register_hotkey(app, "A")?;
    
    Ok(())
}

fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let manager = app.global_shortcut();
    manager.unregister_all().ok();
    manager
        .register(hotkey)
        .map_err(|e| format!("Windows refused the hotkey {hotkey}: {e}"))?;
    let _ = app.emit("hotkey://registered", hotkey.to_string());
    Ok(())
}

pub fn apply(app: &AppHandle, next: &str) -> Result<(), String> {
    let manager = app.global_shortcut();
    let previous = app
        .try_state::<AppState>()
        .map(|state| state.settings().hotkey)
        .unwrap_or_else(|| "Alt+B".to_string());
    manager.unregister_all().ok();
    if let Err(error) = manager.register(next) {
        let _ = manager.register(previous.as_str());
        return Err(format!("Windows refused {next}: {error}"));
    }
    let _ = app.emit("hotkey://registered", next.to_string());
    Ok(())
}

pub fn validate(hotkey: &str) -> Result<(), String> {
    hotkey
        .parse::<Shortcut>()
        .map(|_| ())
        .map_err(|e| format!("That is not a valid hotkey: {e}"))
}

pub fn handle_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if state.snip_active.load(Ordering::SeqCst) {
        return;
    }
    
    // Toggle avatar visibility when 'A' is pressed
    if shortcut.to_string() == "A" {
        if let Some(avatar) = app.get_webview_window("avatar") {
            if avatar.is_visible().unwrap_or(false) {
                let _ = avatar.hide();
            } else {
                let _ = avatar.show();
            }
        }
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::commands::begin_snip(&handle).await;
    });
}
