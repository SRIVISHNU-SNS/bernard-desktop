use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not locate the app data directory: {e}"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create the app data directory: {e}"))?;
    Ok(dir)
}

pub fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = data_dir(app)?.join("models");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create the models directory: {e}"))?;
    Ok(dir)
}

pub fn captures_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = data_dir(app)?.join("captures");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create the capture directory: {e}"))?;
    Ok(dir)
}

pub fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("bernard.sqlite3"))
}

pub fn resource_sidecar(root: &std::path::Path, backend_dir: &str) -> PathBuf {
    root.join("sidecar").join(backend_dir).join("llama-server.exe")
}

pub fn dev_sidecar(backend_dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sidecar")
        .join(backend_dir)
        .join("llama-server.exe")
}

pub fn find_server_binary(app: &AppHandle, backend_dir: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_sidecar(&resource_dir, backend_dir));
        candidates.push(resource_dir.join(backend_dir).join("llama-server.exe"));
        candidates.push(resource_dir.join("llama-server.exe"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("sidecar").join(backend_dir).join("llama-server.exe"));
        }
    }
    candidates.push(dev_sidecar(backend_dir));

    candidates.into_iter().find(|path| path.is_file())
}

pub fn cleanup_old_capture_artifacts(captures: &std::path::Path, keep: &[String]) {
    let Ok(entries) = std::fs::read_dir(captures) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let path_string = path.to_string_lossy().to_string();
        if !path.is_file() || keep.iter().any(|k| k == &path_string) {
            continue;
        }
        let is_png = path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
        if !is_png {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 60 * 60 * 24 * 7);
        if stale {
            let _ = std::fs::remove_file(path);
        }
    }
}
