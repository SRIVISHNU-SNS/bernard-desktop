use crate::capture::CaptureBatch;
use crate::db::Db;
use crate::models::{now_ms, AppSettings, Attachment};
use crate::server::ServerManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

pub struct PendingSnip {
    pub attachment: Attachment,
    pub created_at: i64,
    pub source: String,
}

pub struct AppState {
    pub db: Arc<Db>,
    settings: RwLock<AppSettings>,
    pub server: Arc<ServerManager>,
    pub pending_snip: Mutex<Option<PendingSnip>>,
    pub capture: Mutex<Option<CaptureBatch>>,
    pub streams: Mutex<HashMap<String, CancellationToken>>,
    pub downloads: Mutex<HashMap<String, CancellationToken>>,
    pub active_download: AtomicBool,
    pub snip_active: AtomicBool,
    pub overlay_active: AtomicBool,
    pub panel_visible: AtomicBool,
    pub snip_panel_was_visible: AtomicBool,
    pub menu_visible: AtomicBool,
    pub last_snip_at: AtomicI64,
    pub drag_generation: AtomicU64,
    pub started_at: i64,
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    pub fn replace_settings(&self, settings: AppSettings) {
        if let Ok(mut guard) = self.settings.write() {
            *guard = settings;
        }
    }

    pub fn set_avatar_position(&self, x: i32, y: i32) {
        if let Ok(mut guard) = self.settings.write() {
            guard.avatar_x = Some(x);
            guard.avatar_y = Some(y);
        }
    }

    pub fn note_snip(&self) {
        self.last_snip_at.store(now_ms(), Ordering::SeqCst);
    }

    pub fn register_stream(&self, id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        if let Ok(mut streams) = self.streams.lock() {
            streams.insert(id.to_string(), token.clone());
        }
        token
    }

    pub fn cancel_stream(&self, id: &str) -> bool {
        let token = self
            .streams
            .lock()
            .ok()
            .and_then(|streams| streams.get(id).cloned());
        if let Some(token) = token {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn finish_stream(&self, id: &str) {
        if let Ok(mut streams) = self.streams.lock() {
            streams.remove(id);
        }
    }

    pub fn cancel_all_streams(&self) {
        if let Ok(mut streams) = self.streams.lock() {
            for token in streams.values() {
                token.cancel();
            }
            streams.clear();
        }
    }
}

pub fn initialize(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = crate::paths::data_dir(app)?;
    let models_dir = data_dir.join("models");
    std::fs::create_dir_all(&models_dir)?;
    std::fs::create_dir_all(data_dir.join("captures"))?;

    let db_path = crate::paths::database_path(app)?;
    let db = Db::open(&db_path, &data_dir)?;
    let settings = db.load_settings()?;
    let server = Arc::new(ServerManager::new(app.clone()));
    let state = AppState {
        db: Arc::new(db),
        settings: RwLock::new(settings),
        server: server.clone(),
        pending_snip: Mutex::new(None),
        capture: Mutex::new(None),
        streams: Mutex::new(HashMap::new()),
        downloads: Mutex::new(HashMap::new()),
        active_download: AtomicBool::new(false),
        snip_active: AtomicBool::new(false),
        overlay_active: AtomicBool::new(false),
        panel_visible: AtomicBool::new(false),
        snip_panel_was_visible: AtomicBool::new(false),
        menu_visible: AtomicBool::new(false),
        last_snip_at: AtomicI64::new(0),
        drag_generation: AtomicU64::new(0),
        started_at: now_ms(),
        data_dir: data_dir.clone(),
    };
    app.manage(state);

    let images = app
        .try_state::<AppState>()
        .map(|state| state.db.all_image_paths().unwrap_or_default())
        .unwrap_or_default();
    crate::paths::cleanup_old_capture_artifacts(&data_dir.join("captures"), &images);

    Ok(())
}

pub fn state(app: &AppHandle) -> Result<tauri::State<'_, AppState>, String> {
    app.try_state::<AppState>()
        .ok_or_else(|| "Bernard's backend is still starting.".to_string())
}
