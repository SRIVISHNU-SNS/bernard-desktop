//! Bernard's Tauri backend.
//!
//! The binary lives in `main.rs`; everything else is exposed from this library
//! so integration tests can exercise the database and server helpers.

pub mod app_state;
pub mod capture;
pub mod commands;
pub mod db;
pub mod download;
pub mod error;
pub mod hotkeys;
pub mod models;
pub mod paths;
pub mod server;
pub mod windows;

use tauri::Manager;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("bernard=info")),
        )
        .with_target(false)
        .compact()
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_avatar(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    hotkeys::handle_shortcut(app, shortcut, event);
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::get_settings,
            commands::save_settings,
            commands::list_conversations,
            commands::create_conversation,
            commands::rename_conversation,
            commands::delete_conversation,
            commands::get_messages,
            commands::load_attachments,
            commands::send_message,
            commands::stop_stream,
            commands::regenerate,
            commands::delete_from_message,
            commands::edit_resend,
            commands::copy_text,
            commands::import_clipboard,
            commands::save_dropped_image,
            commands::save_pasted_image,
            commands::open_external,
            commands::snip_now,
            commands::cancel_snip,
            commands::snip_commit,
            commands::overlay_payload,
            commands::toggle_panel,
            commands::show_panel,
            commands::hide_panel,
            commands::show_avatar_menu,
            commands::hide_avatar_menu,
            commands::hide_avatar,
            commands::show_avatar,
            commands::quit_app,
            commands::start_dragging,
            commands::persist_avatar_position,
            commands::server_status,
            commands::restart_server,
            commands::diagnostics,
            commands::model_info,
            commands::download_model,
            commands::cancel_download,
            commands::pick_model_file,
            commands::reset_settings,
            commands::clear_all_data,
            commands::open_data_folder,
            commands::set_theme,
            commands::notify_avatar_state
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            app_state::initialize(&handle)?;
            windows::setup(&handle)?;
            hotkeys::register(&handle)?;
            server::start(&handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .map_err(|e| e.to_string())
        .and_then(|app| {
            app.run(|handle, event| {
                if let tauri::RunEvent::ExitRequested { .. } = event {
                    server::shutdown(handle);
                }
                if let tauri::RunEvent::Exit = event {
                    server::shutdown(handle);
                }
            });
            Ok(())
        })
        .unwrap_or_else(|e| {
            eprintln!("Bernard could not start: {e}");
            std::process::exit(1);
        });
}
