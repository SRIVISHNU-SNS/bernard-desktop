use crate::app_state::AppState;
use crate::models::{preset, preset_mmproj_url, preset_model_url, DownloadProgress};
use crate::paths;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

const RETRIES: u32 = 8;

pub fn start_download(app: AppHandle, preset_id: String) -> Result<(), String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Bernard's backend is still starting.".to_string())?;
    if state.active_download.swap(true, Ordering::SeqCst) {
        return Err("A model download is already in progress.".to_string());
    }
    let preset = preset(&preset_id).clone();
    let cancel = CancellationToken::new();
    if let Ok(mut downloads) = state.downloads.lock() {
        downloads.insert(preset_id.clone(), cancel.clone());
    }
    drop(state);

    tauri::async_runtime::spawn(async move {
        let result = download_preset(&app, &preset_id, cancel.clone()).await;
        if let Some(state) = app.try_state::<AppState>() {
            state.active_download.store(false, Ordering::SeqCst);
            if let Ok(mut downloads) = state.downloads.lock() {
                downloads.remove(&preset_id);
            }
        }
        match result {
            Ok(()) => {
                let _ = app.emit("model://done", preset_id.clone());
                if let Some(state) = app.try_state::<AppState>() {
                    let mut settings = state.settings();
                    if settings.model_preset == preset_id {
                        let dir = state.data_dir.join("models");
                        settings.model_path = dir
                            .join(preset.model_file)
                            .to_string_lossy()
                            .to_string();
                        settings.mmproj_path = dir
                            .join(preset.mmproj_file)
                            .to_string_lossy()
                            .to_string();
                        if state.db.save_settings(&settings).is_ok() {
                            state.replace_settings(settings);
                            crate::server::request_restart(&app);
                        }
                    }
                }
            }
            Err(error) => {
                let _ = app.emit(
                    "model://failed",
                    serde_json::json!({ "presetId": preset_id, "message": error }),
                );
            }
        }
    });
    Ok(())
}

pub fn cancel_download(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        let tokens: Vec<CancellationToken> = state
            .downloads
            .lock()
            .map(|downloads| downloads.values().cloned().collect())
            .unwrap_or_default();
        for token in tokens {
            token.cancel();
        }
    }
}

async fn download_preset(
    app: &AppHandle,
    preset_id: &str,
    cancel: CancellationToken,
) -> Result<(), String> {
    let preset = preset(preset_id);
    let dir = paths::models_dir(app)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create the model folder: {e}"))?;

    let model = dir.join(preset.model_file);
    let mmproj = dir.join(preset.mmproj_file);
    let model_url = preset_model_url(preset);
    let mmproj_url = preset_mmproj_url(preset);

    if !verified(&model, preset.model_bytes, preset.model_sha256, cancel.clone()).await? {
        fetch_file(
            app,
            preset_id,
            &model_url,
            &model,
            preset.model_bytes,
            preset.model_sha256,
            cancel.clone(),
        )
        .await?;
    }
    if !verified(
        &mmproj,
        preset.mmproj_bytes,
        preset.mmproj_sha256,
        cancel.clone(),
    )
    .await?
    {
        fetch_file(
            app,
            preset_id,
            &mmproj_url,
            &mmproj,
            preset.mmproj_bytes,
            preset.mmproj_sha256,
            cancel.clone(),
        )
        .await?;
    }
    Ok(())
}

async fn verified(
    path: &Path,
    expected_bytes: u64,
    expected_hash: &str,
    cancel: CancellationToken,
) -> Result<bool, String> {
    let meta = match tokio::fs::metadata(path).await {
        Ok(meta) => meta,
        Err(_) => return Ok(false),
    };
    if meta.len() != expected_bytes {
        return Ok(false);
    }
    verify_hash(path, expected_hash, cancel).await
}

async fn verify_hash(
    path: &Path,
    expected_hash: &str,
    cancel: CancellationToken,
) -> Result<bool, String> {
    let mut file =
        tokio::fs::File::open(path).await.map_err(|e| format!("Could not open the model file: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        if cancel.is_cancelled() {
            return Err("The download was cancelled.".to_string());
        }
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|e| format!("Could not verify the model file: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = format!("{:x}", hasher.finalize());
    Ok(digest.eq_ignore_ascii_case(expected_hash))
}

async fn fetch_file(
    app: &AppHandle,
    preset_id: &str,
    url: &str,
    destination: &Path,
    total: u64,
    expected_hash: &str,
    cancel: CancellationToken,
) -> Result<(), String> {
    let part = PathBuf::from(format!("{}.part", destination.display()));
    let mut last_error = String::new();

    for attempt in 1..=RETRIES {
        if cancel.is_cancelled() {
            return Err("The download was cancelled.".to_string());
        }
        match download_once(
            app,
            preset_id,
            url,
            &part,
            total,
            &cancel,
        )
        .await
        {
            Ok(()) => {
                emit_progress(
                    app,
                    preset_id,
                    destination
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("model"),
                    total,
                    total,
                    true,
                    None,
                );
                let hash_ok = verify_hash(&part, expected_hash, cancel.clone()).await?;
                if !hash_ok {
                    let _ = tokio::fs::remove_file(&part).await;
                    last_error =
                        "The downloaded file failed its integrity check and was removed.".to_string();
                    if attempt == RETRIES {
                        return Err(last_error);
                    }
                    continue;
                }
                tokio::fs::rename(&part, destination)
                    .await
                    .map_err(|e| format!("Could not finish the model download: {e}"))?;
                return Ok(());
            }
            Err(error) => {
                if cancel.is_cancelled() {
                    return Err("The download was cancelled.".to_string());
                }
                last_error = error;
                if attempt < RETRIES {
                    tokio::time::sleep(Duration::from_secs(2 * attempt as u64)).await;
                }
            }
        }
    }
    Err(format!(
        "The download could not finish after {RETRIES} attempts. The partial file was kept so it can resume. {last_error}"
    ))
}

async fn download_once(
    app: &AppHandle,
    preset_id: &str,
    url: &str,
    part: &Path,
    total: u64,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let existing = tokio::fs::metadata(part)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0)
        .min(total);
    // No total request timeout: model files take many minutes on slow links. The
    // read timeout only fires when the host stops sending data mid-transfer.
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| format!("Could not create the download client: {e}"))?;
    let mut request = client.get(url);
    if existing > 0 {
        request = request.header("Range", format!("bytes={existing}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("Could not reach the model host: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "The model host returned HTTP {} for this download.",
            response.status().as_u16()
        ));
    }

    let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let start = if resumed { existing } else { 0 };
    let mut file = if resumed {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(part)
            .await
    } else {
        tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(part)
            .await
    }
    .map_err(|e| format!("Could not open the partial model file: {e}"))?;

    let mut downloaded = start;
    let file_label = part
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("model")
        .trim_end_matches(".part")
        .to_string();
    emit_progress(
        app,
        preset_id,
        &file_label,
        downloaded,
        total,
        false,
        None,
    );

    let mut stream = response.bytes_stream();
    let mut last_emit = Instant::now();
    let mut last_bytes = downloaded;
    while let Some(chunk) = tokio::select! {
        _ = cancel.cancelled() => {
            drop(file);
            return Err("The download was cancelled.".to_string());
        }
        next = stream.next() => next,
    } {
        let chunk = chunk.map_err(|e| format!("The model download stopped: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Could not write the model file: {e}"))?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() > Duration::from_millis(250) {
            let seconds = last_emit.elapsed().as_secs_f64();
            let speed = if seconds > 0.0 {
                ((downloaded.saturating_sub(last_bytes)) as f64 / seconds) as u64
            } else {
                0
            };
            emit_progress(
                app,
                preset_id,
                &file_label,
                downloaded.min(total),
                total,
                false,
                Some(speed),
            );
            last_emit = Instant::now();
            last_bytes = downloaded;
        }
    }
    file.flush()
        .await
        .map_err(|e| format!("Could not flush the model file: {e}"))?;
    let final_size = tokio::fs::metadata(part)
        .await
        .map(|meta| meta.len())
        .unwrap_or(downloaded);
    if final_size != total {
        return Err(format!(
            "The download ended early ({final_size} of {total} bytes)."
        ));
    }
    emit_progress(app, preset_id, &file_label, total, total, false, Some(0));
    Ok(())
}

fn emit_progress(
    app: &AppHandle,
    preset_id: &str,
    file: &str,
    downloaded: u64,
    total: u64,
    done: bool,
    speed: Option<u64>,
) {
    let progress = DownloadProgress {
        preset_id: preset_id.to_string(),
        file: file.to_string(),
        downloaded,
        total,
        percent: if total > 0 {
            (downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        },
        speed_bytes_per_second: speed.unwrap_or(0),
        error: None,
        done,
    };
    let _ = app.emit("model://progress", progress);
}

pub async fn model_file_info(app: &AppHandle) -> Result<crate::models::ModelFileInfo, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Bernard's backend is still starting.".to_string())?;
    Ok(crate::models::ModelFileInfo::for_settings(&state.settings()))
}
