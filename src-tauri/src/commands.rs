use crate::app_state::{self, AppState};
use crate::capture::{self, Selection};
use crate::models::*;
use crate::paths;
use crate::server::{self, build_chat_body};
use crate::windows;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

const MAX_SNIP_IMAGES: usize = 4;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnipSelection {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4())
}

fn title_for(content: &str) -> String {
    let clean = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() {
        "Snip".to_string()
    } else {
        clean.chars().take(52).collect()
    }
}

fn managed_image(app: &AppHandle, path: &str) -> bool {
    let Ok(dir) = paths::captures_dir(app) else {
        return false;
    };
    let Ok(dir) = std::fs::canonicalize(dir) else {
        return false;
    };
    let Ok(candidate) = std::fs::canonicalize(path) else {
        return false;
    };
    candidate.starts_with(dir)
}

fn validate_images(app: &AppHandle, paths: &[String]) -> Result<Vec<String>, String> {
    let sanitized = crate::db::sanitize_image_paths(paths);
    if sanitized.len() > MAX_SNIP_IMAGES {
        return Err(format!("Attach at most {MAX_SNIP_IMAGES} images."));
    }
    for path in &sanitized {
        if !managed_image(app, path) {
            return Err("That image is not one of Bernard's managed snips.".to_string());
        }
    }
    Ok(sanitized)
}

fn settle_avatar(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(2200)).await;
        let busy = handle
            .try_state::<AppState>()
            .map(|state| {
                state
                    .streams
                    .lock()
                    .map(|streams| !streams.is_empty())
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if busy {
            return;
        }
        let visible = windows::panel_visible(&handle);
        let _ = handle.emit("avatar://state", if visible { "listening" } else { "idle" });
    });
}

#[tauri::command]
pub async fn bootstrap(app: AppHandle, state: State<'_, AppState>) -> Result<Bootstrap, String> {
    let settings = state.settings();
    let conversations = state.db.list_conversations("")?;
    let server = state.server.status().await;
    Ok(Bootstrap {
        model: ModelFileInfo::for_settings(&settings),
        settings,
        server,
        conversations,
        app_version: crate::VERSION.to_string(),
        data_dir: state.data_dir.to_string_lossy().to_string(),
        started_at: state.started_at,
    })
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    settings.validate()?;
    crate::hotkeys::validate(&settings.hotkey)?;
    let previous = state.settings();
    let hotkey_changed = previous.hotkey != settings.hotkey;
    let model_changed = previous.model_path != settings.model_path
        || previous.mmproj_path != settings.mmproj_path
        || previous.backend != settings.backend
        || previous.model_preset != settings.model_preset;

    let mut settings = settings;
    if settings.start_with_windows != previous.start_with_windows {
        if let Err(error) = apply_autostart(&app, settings.start_with_windows) {
            settings.start_with_windows = previous.start_with_windows;
            return Err(error);
        }
    }

    state.db.save_settings(&settings)?;
    state.replace_settings(settings.clone());
    if hotkey_changed {
        crate::hotkeys::apply(&app, &settings.hotkey)?;
    }
    if previous.avatar_size != settings.avatar_size {
        windows::apply_avatar_size(&app, settings.avatar_size);
    }
    if model_changed {
        server::request_restart(&app);
    }
    windows::apply_theme(&app);
    let _ = app.emit("settings://changed", &settings);
    Ok(settings)
}

fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let current = manager.is_enabled().unwrap_or(false);
    if enabled && !current {
        manager
            .enable()
            .map_err(|e| format!("Could not enable start with Windows: {e}"))?;
    } else if !enabled && current {
        manager
            .disable()
            .map_err(|e| format!("Could not disable start with Windows: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_conversations(
    state: State<'_, AppState>,
    search: String,
) -> Result<Vec<Conversation>, String> {
    state.db.list_conversations(&search)
}

#[tauri::command]
pub fn create_conversation(state: State<'_, AppState>) -> Result<Conversation, String> {
    state.db.create_conversation("New chat")
}

#[tauri::command]
pub fn rename_conversation(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("Give the conversation a name.".to_string());
    }
    state.db.rename_conversation(&id, &title)
}

#[tauri::command]
pub fn delete_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let images = state.db.delete_conversation(&id)?;
    capture::remove_capture_files(&images);
    let _ = app.emit("conversations://changed", ());
    Ok(())
}

#[tauri::command]
pub fn get_messages(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<StoredMessage>, String> {
    state.db.messages(&conversation_id)
}

#[tauri::command]
pub fn load_attachments(
    app: AppHandle,
    paths: Vec<String>,
) -> Result<Vec<Attachment>, String> {
    let mut attachments = Vec::new();
    for path in paths {
        if !managed_image(&app, &path) {
            continue;
        }
        if let Ok(attachment) = capture::attachment_from_path(Path::new(&path)) {
            attachments.push(attachment);
        }
    }
    Ok(attachments)
}

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SendMessageRequest,
) -> Result<SendMessageResult, String> {
    let content = request.content.trim().to_string();
    let image_paths = validate_images(&app, &request.image_paths)?;
    if content.is_empty() && image_paths.is_empty() {
        return Err("Type a message or attach a snip first.".to_string());
    }

    let conversation = match request.conversation_id {
        Some(id) => state.db.get_conversation(&id)?,
        None => state
            .db
            .create_conversation(&title_for(&content))?,
    };
    let user_message = StoredMessage {
        id: new_id("msg"),
        conversation_id: conversation.id.clone(),
        role: "user".to_string(),
        content: content.clone(),
        image_paths: image_paths.clone(),
        created_at: now_ms(),
        ttft_ms: None,
        tokens_per_second: None,
        prompt_tokens: None,
        completion_tokens: None,
        cache_tokens: None,
    };
    state.db.insert_message(&user_message)?;
    state
        .db
        .maybe_set_title_from_message(&conversation.id, &content)?;
    if !image_paths.is_empty() {
        if let Ok(mut pending) = state.pending_snip.lock() {
            *pending = None;
        }
    }

    let request_id = new_id("req");
    let cancel = state.register_stream(&request_id);
    let handle = app.clone();
    let conversation_id = conversation.id.clone();
    let spawned_request = request_id.clone();
    tauri::async_runtime::spawn(async move {
        launch_completion(handle, conversation_id, spawned_request, cancel).await;
    });

    let _ = app.emit("conversations://changed", ());
    Ok(SendMessageResult {
        conversation_id: conversation.id,
        user_message,
        request_id,
    })
}

#[tauri::command]
pub fn stop_stream(state: State<'_, AppState>, request_id: String) -> bool {
    state.cancel_stream(&request_id)
}

#[tauri::command]
pub async fn regenerate(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<SendMessageResult, String> {
    state.cancel_all_streams();
    state.db.delete_trailing_assistant(&conversation_id)?;
    let messages = state.db.messages(&conversation_id)?;
    let user = messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .cloned()
        .ok_or_else(|| "There is no question to regenerate.".to_string())?;
    let request_id = new_id("req");
    let cancel = state.register_stream(&request_id);
    let handle = app.clone();
    let spawned_request = request_id.clone();
    let spawned_conversation = conversation_id.clone();
    tauri::async_runtime::spawn(async move {
        launch_completion(handle, spawned_conversation, spawned_request, cancel).await;
    });
    Ok(SendMessageResult {
        conversation_id,
        user_message: user,
        request_id,
    })
}

#[tauri::command]
pub async fn edit_resend(
    app: AppHandle,
    state: State<'_, AppState>,
    message_id: String,
    content: String,
) -> Result<SendMessageResult, String> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("The edited message cannot be empty.".to_string());
    }
    state.cancel_all_streams();
    let existing = state.db.message_by_id(&message_id)?;
    if existing.role != "user" {
        return Err("Only your own messages can be edited.".to_string());
    }
    state.db.update_message_content(&message_id, &content)?;
    state.db.delete_after_message(&message_id)?;
    let user = state.db.message_by_id(&message_id)?;
    let request_id = new_id("req");
    let cancel = state.register_stream(&request_id);
    let handle = app.clone();
    let conversation_id = existing.conversation_id.clone();
    let spawned_request = request_id.clone();
    tauri::async_runtime::spawn(async move {
        launch_completion(handle, conversation_id, spawned_request, cancel).await;
    });
    let _ = app.emit("conversations://changed", ());
    Ok(SendMessageResult {
        conversation_id: existing.conversation_id,
        user_message: user,
        request_id,
    })
}

#[tauri::command]
pub fn delete_from_message(state: State<'_, AppState>, message_id: String) -> Result<(), String> {
    state.db.delete_from_message(&message_id)
}

async fn launch_completion(
    app: AppHandle,
    conversation_id: String,
    request_id: String,
    cancel: CancellationToken,
) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let settings = state.settings();
    let history = match state.db.messages(&conversation_id) {
        Ok(messages) => messages,
        Err(error) => {
            emit_failure(&app, &request_id, &conversation_id, &error, "", true);
            state.finish_stream(&request_id);
            return;
        }
    };
    let (messages, trimmed) = build_api_messages(&app, &settings, &history);
    let body = build_chat_body(messages, settings.max_tokens, settings.temperature);

    let _ = app.emit("avatar://state", "thinking");
    let handle = app.clone();
    let request = request_id.clone();
    let conversation = conversation_id.clone();
    let mut first = true;
    let started = Instant::now();
    let result = state
        .server
        .chat_stream(body, cancel.clone(), |delta| {
            if first {
                first = false;
                let _ = handle.emit("avatar://state", "answering");
            }
            let _ = handle.emit(
                "chat://delta",
                ChatDelta {
                    request_id: request.clone(),
                    conversation_id: conversation.clone(),
                    delta: delta.to_string(),
                },
            );
        })
        .await;

    match result {
        Ok(outcome) => {
            let total_ms = started.elapsed().as_secs_f64() * 1000.0;
            if outcome.content.trim().is_empty() {
                let message = if outcome.cancelled {
                    "Stopped before Bernard answered.".to_string()
                } else {
                    "Bernard returned an empty answer. Try asking again or rephrase the question."
                        .to_string()
                };
                emit_failure(&app, &request_id, &conversation_id, &message, "", true);
                state.finish_stream(&request_id);
                settle_avatar(&app);
                return;
            }

            let assistant = StoredMessage {
                id: new_id("msg"),
                conversation_id: conversation_id.clone(),
                role: "assistant".to_string(),
                content: outcome.content.clone(),
                image_paths: Vec::new(),
                created_at: now_ms(),
                ttft_ms: outcome.timing.ttft_ms.map(|value| value as i64),
                tokens_per_second: outcome.timing.tokens_per_second,
                prompt_tokens: outcome.timing.prompt_tokens,
                completion_tokens: outcome.timing.completion_tokens,
                cache_tokens: outcome.timing.cache_tokens,
            };
            if let Err(error) = state.db.insert_message(&assistant) {
                emit_failure(
                    &app,
                    &request_id,
                    &conversation_id,
                    &format!("Bernard answered, but the answer could not be saved: {error}"),
                    &outcome.content,
                    true,
                );
                state.finish_stream(&request_id);
                return;
            }

            if let Some(ttft) = outcome.timing.ttft_ms {
                let _ = state.db.record_performance(
                    "time_to_first_token",
                    ttft,
                    Some(&conversation_id),
                );
            }
            if let Some(tps) = outcome.timing.tokens_per_second {
                let _ = state.db.record_performance(
                    "tokens_per_second",
                    tps,
                    Some(&conversation_id),
                );
            }
            let _ = state.db.record_performance(
                "response_total",
                total_ms,
                Some(
                    format!(
                        "{} prompt / {} completion / {} cached",
                        outcome.timing.prompt_tokens.unwrap_or(0),
                        outcome.timing.completion_tokens.unwrap_or(0),
                        outcome.timing.cache_tokens.unwrap_or(0)
                    )
                    .as_str(),
                ),
            );

            if outcome.cancelled {
                emit_failure(
                    &app,
                    &request_id,
                    &conversation_id,
                    "Stopped.",
                    &outcome.content,
                    true,
                );
            } else {
                let _ = app.emit(
                    "chat://done",
                    ChatDone {
                        request_id: request_id.clone(),
                        conversation_id: conversation_id.clone(),
                        message: assistant,
                        context_trimmed: trimmed,
                    },
                );
            }
            let _ = app.emit(
                "server://metrics",
                json!({
                    "ttftMs": outcome.timing.ttft_ms,
                    "tokensPerSecond": outcome.timing.tokens_per_second,
                    "promptTokens": outcome.timing.prompt_tokens,
                    "completionTokens": outcome.timing.completion_tokens,
                    "cacheTokens": outcome.timing.cache_tokens,
                    "totalMs": total_ms,
                    "conversationId": conversation_id,
                }),
            );
            let _ = app.emit("conversations://changed", ());
            state.finish_stream(&request_id);
            settle_avatar(&app);
        }
        Err(error) => {
            emit_failure(&app, &request_id, &conversation_id, &error, "", true);
            state.finish_stream(&request_id);
            let _ = app.emit("avatar://state", "error");
            state.server.push_log(format!("chat failed: {error}")).await;
            settle_avatar(&app);
        }
    }
}

fn emit_failure(
    app: &AppHandle,
    request_id: &str,
    conversation_id: &str,
    message: &str,
    partial: &str,
    retryable: bool,
) {
    if !partial.is_empty() {
        if let Some(state) = app.try_state::<AppState>() {
            let assistant = StoredMessage {
                id: new_id("msg"),
                conversation_id: conversation_id.to_string(),
                role: "assistant".to_string(),
                content: partial.to_string(),
                image_paths: Vec::new(),
                created_at: now_ms(),
                ttft_ms: None,
                tokens_per_second: None,
                prompt_tokens: None,
                completion_tokens: None,
                cache_tokens: None,
            };
            let _ = state.db.insert_message(&assistant);
        }
    }
    let _ = app.emit(
        "chat://failed",
        ChatFailed {
            request_id: request_id.to_string(),
            conversation_id: conversation_id.to_string(),
            message: message.to_string(),
            partial: partial.to_string(),
            retryable,
        },
    );
}

fn estimate_tokens(message: &StoredMessage) -> usize {
    let text_tokens = (message.content.chars().count() as f64 / 4.0).ceil() as usize;
    text_tokens + message.image_paths.len() * 620 + 8
}

fn build_api_messages(
    app: &AppHandle,
    settings: &AppSettings,
    history: &[StoredMessage],
) -> (Vec<Value>, bool) {
    let mut trimmed = false;
    let budget = (DEFAULT_CONTEXT as usize)
        .saturating_sub(settings.max_tokens as usize)
        .saturating_sub(256);
    let mut kept: Vec<&StoredMessage> = history.iter().collect();
    while kept.len() > 2
        && kept.iter().map(|message| estimate_tokens(message)).sum::<usize>() > budget
    {
        let pinned = history
            .iter()
            .find(|message| message.role == "user" && !message.image_paths.is_empty())
            .map(|message| message.id.clone());
        let removable = kept.iter().position(|message| {
            Some(&message.id) != pinned.as_ref() && message.role != "system"
        });
        let Some(index) = removable else {
            break;
        };
        if kept.len() <= 2 {
            break;
        }
        kept.remove(index);
        trimmed = true;
    }
    if kept.len() == history.len() && history.iter().map(|message| estimate_tokens(message)).sum::<usize>() > budget {
        trimmed = true;
    }

    let mut messages = vec![json!({
        "role": "system",
        "content": settings.system_prompt
    })];
    for message in kept {
        if message.role == "system" {
            continue;
        }
        if message.role == "user" && !message.image_paths.is_empty() {
            let mut parts: Vec<Value> = Vec::new();
            let text = if message.content.trim().is_empty() {
                "Explain this."
            } else {
                message.content.as_str()
            };
            parts.push(json!({ "type": "text", "text": text }));
            for path in &message.image_paths {
                if !managed_image(app, path) {
                    continue;
                }
                if let Ok(attachment) = capture::attachment_from_path(Path::new(path)) {
                    parts.push(json!({
                        "type": "image_url",
                        "image_url": { "url": attachment.data_url }
                    }));
                }
            }
            messages.push(json!({ "role": "user", "content": parts }));
        } else if message.role == "user" || message.role == "assistant" {
            if message.content.is_empty() {
                continue;
            }
            messages.push(json!({
                "role": message.role,
                "content": message.content
            }));
        }
    }
    (messages, trimmed)
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    capture::copy_text(&text)
}

#[tauri::command]
pub fn import_clipboard(app: AppHandle) -> Result<Attachment, String> {
    capture::import_clipboard(&app)
}

#[tauri::command]
pub fn save_dropped_image(app: AppHandle, path: String) -> Result<Attachment, String> {
    let candidate = PathBuf::from(&path);
    if !candidate.is_file() {
        return Err("That dropped file could not be found.".to_string());
    }
    if candidate
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
            )
        })
        != Some(true)
    {
        return Err("Drop a PNG, JPEG, WebP, GIF, or BMP image.".to_string());
    }
    capture::save_pending_image(&app, &candidate)
}

#[tauri::command]
pub fn save_pasted_image(app: AppHandle, data_url: String) -> Result<Attachment, String> {
    if !data_url.starts_with("data:image/") {
        return Err("The pasted data was not an image.".to_string());
    }
    capture::save_data_url(&app, &data_url, "paste")
}

#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    let target = url.trim();
    if target.len() > 4096 {
        return Err("That link is too long to open.".to_string());
    }
    let lower = target.to_ascii_lowercase();
    let allowed = ["https://", "http://", "mailto:"];
    if !allowed.iter().any(|prefix| lower.starts_with(prefix)) {
        return Err("Bernard only opens web and email links.".to_string());
    }

    #[cfg(windows)]
    {
        use ::windows::core::PCWSTR;
        use ::windows::Win32::UI::Shell::ShellExecuteW;
        use ::windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR::null(),
                PCWSTR(wide.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize <= 32 {
            return Err("Windows could not open that link.".to_string());
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        Err("Opening links is only supported on Windows.".to_string())
    }
}

#[tauri::command]
pub async fn snip_now(app: AppHandle) -> Result<(), String> {
    begin_snip(&app).await
}

pub async fn begin_snip(app: &AppHandle) -> Result<(), String> {
    let Some(state) = app.try_state::<AppState>() else {
        return Err("Bernard's backend is still starting.".to_string());
    };
    if state.snip_active.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let started = Instant::now();
    windows::hide_menu(app);
    let panel_was_visible = windows::panel_visible(app);
    state
        .snip_panel_was_visible
        .store(panel_was_visible, Ordering::SeqCst);
    windows::hide_panel(app);

    let capture_result = tauri::async_runtime::spawn_blocking(capture::capture_all).await;
    let batch = match capture_result {
        Ok(Ok(batch)) => batch,
        Ok(Err(error)) => {
            state.snip_active.store(false, Ordering::SeqCst);
            let _ = app.emit("snip://failed", json!({ "message": error }));
            if panel_was_visible {
                let _ = windows::show_panel(app);
            }
            return Err(error);
        }
        Err(error) => {
            state.snip_active.store(false, Ordering::SeqCst);
            let message = format!("The screen capture task failed: {error}");
            let _ = app.emit("snip://failed", json!({ "message": message }));
            if panel_was_visible {
                let _ = windows::show_panel(app);
            }
            return Err(message);
        }
    };

    let count = batch.monitors.len();
    if count == 0 {
        state.snip_active.store(false, Ordering::SeqCst);
        let message = "No displays were available to snip.".to_string();
        let _ = app.emit("snip://failed", json!({ "message": message }));
        return Err(message);
    }
    windows::ensure_overlay_count(app, count)?;

    let payloads: Vec<(String, capture::OverlayPayload, i32, i32, u32, u32)> = batch
        .monitors
        .iter()
        .map(|monitor| {
            (
                monitor.label.clone(),
                capture::payload_for(monitor),
                monitor.x,
                monitor.y,
                monitor.width,
                monitor.height,
            )
        })
        .collect();

    if let Ok(mut slot) = state.capture.lock() {
        *slot = Some(batch);
    }
    state.overlay_active.store(true, Ordering::SeqCst);

    for (index, (label, payload, x, y, width, height)) in payloads.into_iter().enumerate() {
        let _ = app.emit_to(&label, "snip://show", payload);
        if let Err(error) = windows::show_overlay(app, &label, x, y, width, height, index == 0) {
            windows::hide_overlays(app);
            if let Ok(mut slot) = state.capture.lock() {
                *slot = None;
            }
            state.snip_active.store(false, Ordering::SeqCst);
            let _ = app.emit("snip://failed", json!({ "message": error }));
            if panel_was_visible {
                let _ = windows::show_panel(app);
            }
            return Err(error);
        }
    }

    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let detail = format!("{count} display(s)");
    let _ = state
        .db
        .record_performance("hotkey_to_overlay", elapsed, Some(detail.as_str()));
    let _ = app.emit("performance://sample", json!({ "kind": "hotkey_to_overlay", "valueMs": elapsed }));
    Ok(())
}

#[tauri::command]
pub fn overlay_payload(
    state: State<'_, AppState>,
    label: String,
) -> Result<capture::OverlayPayload, String> {
    let slot = state
        .capture
        .lock()
        .map_err(|_| "The snip state was unavailable.".to_string())?;
    let batch = slot
        .as_ref()
        .ok_or_else(|| "There is no active snip.".to_string())?;
    batch
        .monitors
        .iter()
        .find(|monitor| monitor.label == label)
        .map(capture::payload_for)
        .ok_or_else(|| "That display is no longer part of the snip.".to_string())
}

#[tauri::command]
pub fn cancel_snip(app: AppHandle, state: State<'_, AppState>) {
    if !state.overlay_active.load(Ordering::SeqCst) {
        return;
    }
    windows::hide_overlays(&app);
    if let Ok(mut slot) = state.capture.lock() {
        *slot = None;
    }
    state.snip_active.store(false, Ordering::SeqCst);
    let restore = state.snip_panel_was_visible.load(Ordering::SeqCst);
    if restore {
        let _ = windows::show_panel(&app);
    } else {
        let _ = app.emit("avatar://state", "idle");
    }
    let _ = app.emit("snip://cancelled", ());
}

#[tauri::command]
pub async fn snip_commit(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: SnipSelection,
) -> Result<(), String> {
    if !state.overlay_active.load(Ordering::SeqCst) {
        return Err("There is no active snip.".to_string());
    }
    let started = Instant::now();
    let monitor = {
        let slot = state
            .capture
            .lock()
            .map_err(|_| "The snip state was unavailable.".to_string())?;
        let batch = slot
            .as_ref()
            .ok_or_else(|| "There is no active snip.".to_string())?;
        batch
            .monitors
            .iter()
            .find(|monitor| monitor.label == selection.label)
            .cloned()
            .ok_or_else(|| "That display is no longer part of the snip.".to_string())?
    };
    let picked = Selection {
        label: selection.label.clone(),
        x: selection.x,
        y: selection.y,
        width: selection.width,
        height: selection.height,
    };
    let handle = app.clone();
    let crop = tauri::async_runtime::spawn_blocking(move || {
        capture::crop_selection(&handle, &monitor, &picked)
    })
    .await;

    let attachment = match crop {
        Ok(Ok(attachment)) => attachment,
        Ok(Err(error)) => {
            finish_snip(&app, &state);
            let _ = app.emit("snip://failed", json!({ "message": error }));
            let _ = windows::show_panel(&app);
            return Err(error);
        }
        Err(error) => {
            finish_snip(&app, &state);
            let message = format!("Could not crop the snip: {error}");
            let _ = app.emit("snip://failed", json!({ "message": message }));
            let _ = windows::show_panel(&app);
            return Err(message);
        }
    };

    let panel_was_visible = state.snip_panel_was_visible.load(Ordering::SeqCst);
    let source = Path::new(&attachment.path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("snip")
        .to_string();
    if let Ok(mut pending) = state.pending_snip.lock() {
        *pending = Some(crate::app_state::PendingSnip {
            attachment: attachment.clone(),
            created_at: now_ms(),
            source: source.clone(),
        });
    }
    state.note_snip();
    finish_snip(&app, &state);
    windows::show_panel(&app)?;
    let _ = app.emit("panel://focus-composer", ());
    let _ = app.emit(
        "snip://ready",
        SnipReady {
            attachment,
            source,
            panel_was_visible,
        },
    );
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    let _ = state
        .db
        .record_performance("snip_to_panel", elapsed, Some("crop, downscale, save, show"));
    let _ = app.emit("performance://sample", json!({ "kind": "snip_to_panel", "valueMs": elapsed }));
    Ok(())
}

fn finish_snip(app: &AppHandle, state: &AppState) {
    windows::hide_overlays(app);
    if let Ok(mut slot) = state.capture.lock() {
        *slot = None;
    }
    state.overlay_active.store(false, Ordering::SeqCst);
    state.snip_active.store(false, Ordering::SeqCst);
}

#[tauri::command]
pub fn toggle_panel(app: AppHandle) {
    windows::toggle_panel(&app);
}

#[tauri::command]
pub fn show_panel(app: AppHandle) -> Result<(), String> {
    windows::show_panel(&app)
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    windows::hide_panel(&app);
}

#[tauri::command]
pub fn show_avatar_menu(app: AppHandle) -> Result<(), String> {
    windows::toggle_menu(&app)
}

#[tauri::command]
pub fn hide_avatar_menu(app: AppHandle) {
    windows::hide_menu(&app);
}

#[tauri::command]
pub fn hide_avatar(app: AppHandle) {
    windows::hide_avatar(&app);
}

#[tauri::command]
pub fn show_avatar(app: AppHandle) {
    windows::show_avatar(&app);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    windows::persist_state(&app);
    if let Some(state) = app.try_state::<AppState>() {
        state.cancel_all_streams();
    }
    server::shutdown(&app);
    app.exit(0);
}

#[tauri::command]
pub fn start_dragging(window: tauri::WebviewWindow) -> Result<(), String> {
    window
        .start_dragging()
        .map_err(|e| format!("Could not start dragging the avatar: {e}"))
}

#[tauri::command]
pub fn persist_avatar_position(app: AppHandle) {
    windows::persist_state(&app);
}

#[tauri::command]
pub async fn server_status(state: State<'_, AppState>) -> Result<ServerStatus, String> {
    Ok(state.server.status().await)
}

#[tauri::command]
pub fn restart_server(app: AppHandle) {
    server::request_restart(&app);
}

#[tauri::command]
pub async fn diagnostics(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Diagnostics, String> {
    let settings = state.settings();
    let server = state.server.status().await;
    let model = ModelFileInfo::for_settings(&settings);
    let logs = state.server.logs().await;
    let performance = state.db.performance(60)?;
    let last_tokens_per_second = state.db.last_tokens_per_second()?;
    Ok(Diagnostics {
        server,
        model,
        app_version: crate::VERSION.to_string(),
        llama_build: SERVER_BUILD.to_string(),
        os: os_description(),
        cpu: std::env::var("PROCESSOR_IDENTIFIER")
            .unwrap_or_else(|_| "Unknown CPU".to_string()),
        memory_total_bytes: memory_total(),
        process_memory_bytes: process_memory(&app),
        process_cpu_percent: process_cpu_percent(),
        recent_logs: logs,
        performance,
        last_tokens_per_second,
        data_dir: state.data_dir.to_string_lossy().to_string(),
    })
}

fn os_description() -> String {
    format!(
        "{} {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::var("OS").unwrap_or_else(|_| "Windows".to_string())
    )
}

#[cfg(windows)]
fn memory_total() -> u64 {
    use ::windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_ok() {
        status.ullTotalPhys
    } else {
        0
    }
}

#[cfg(not(windows))]
fn memory_total() -> u64 {
    0
}

#[cfg(windows)]
fn process_memory(_app: &AppHandle) -> u64 {
    use ::windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use ::windows::Win32::System::Threading::GetCurrentProcess;
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    if unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    }
    .is_ok()
    {
        counters.WorkingSetSize as u64
    } else {
        0
    }
}

#[cfg(not(windows))]
fn process_memory(_app: &AppHandle) -> u64 {
    0
}

#[cfg(windows)]
fn process_cpu_percent() -> f64 {
    use std::sync::Mutex;
    use ::windows::Win32::Foundation::FILETIME;
    use ::windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    static SAMPLE: Mutex<Option<(Instant, u64)>> = Mutex::new(None);
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    }
    .is_ok();
    if !ok {
        return 0.0;
    }
    let hundred_ns = |time: FILETIME| {
        ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
    };
    let total = hundred_ns(kernel) + hundred_ns(user);
    let now = Instant::now();
    let Ok(mut guard) = SAMPLE.lock() else {
        return 0.0;
    };
    let result = if let Some((previous_time, previous_total)) = *guard {
        let wall = now.duration_since(previous_time).as_secs_f64();
        if wall > 0.05 {
            let delta = total.saturating_sub(previous_total) as f64;
            delta / 10_000_000.0 / wall * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    };
    *guard = Some((now, total));
    result.clamp(0.0, 100.0 * 16.0)
}

#[cfg(not(windows))]
fn process_cpu_percent() -> f64 {
    0.0
}

#[tauri::command]
pub async fn model_info(state: State<'_, AppState>) -> Result<ModelFileInfo, String> {
    Ok(ModelFileInfo::for_settings(&state.settings()))
}

#[tauri::command]
pub fn download_model(app: AppHandle, preset_id: String) -> Result<(), String> {
    crate::download::start_download(app, preset_id)
}

#[tauri::command]
pub fn cancel_download(app: AppHandle) {
    crate::download::cancel_download(&app);
}

#[tauri::command]
pub async fn pick_model_file(kind: String) -> Result<Option<String>, String> {
    let title = if kind == "mmproj" {
        "Choose the multimodal projector (mmproj GGUF)"
    } else {
        "Choose the Gemma 4 model (GGUF)"
    };
    let picked = tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title(title)
            .add_filter("GGUF model", &["gguf"])
            .pick_file()
    })
    .await
    .map_err(|e| format!("The file picker failed: {e}"))?;
    Ok(picked.map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn reset_settings(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    let previous = state.settings();
    let mut defaults = AppSettings::new(&state.data_dir);
    defaults.avatar_x = previous.avatar_x;
    defaults.avatar_y = previous.avatar_y;
    defaults.avatar_size = previous.avatar_size;
    if defaults.start_with_windows != previous.start_with_windows {
        apply_autostart(&app, defaults.start_with_windows)?;
    }
    crate::hotkeys::apply(&app, &defaults.hotkey)?;
    state.db.save_settings(&defaults)?;
    state.replace_settings(defaults.clone());
    windows::apply_theme(&app);
    server::request_restart(&app);
    let _ = app.emit("settings://changed", &defaults);
    Ok(defaults)
}

#[tauri::command]
pub fn clear_all_data(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_all_streams();
    let images = state.db.clear_all()?;
    capture::remove_capture_files(&images);
    if let Ok(mut pending) = state.pending_snip.lock() {
        if let Some(snip) = pending.take() {
            capture::remove_capture_files(&[snip.attachment.path]);
        }
    }
    if let Ok(entries) = std::fs::read_dir(state.data_dir.join("captures")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let _ = app.emit("conversations://changed", ());
    let _ = app.emit("data://cleared", ());
    Ok(())
}

#[tauri::command]
pub fn open_data_folder(state: State<'_, AppState>) -> Result<(), String> {
    let target = state.data_dir.to_string_lossy().to_string();
    std::process::Command::new("explorer.exe")
        .arg(&target)
        .spawn()
        .map_err(|e| format!("Could not open the data folder: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn set_theme(
    app: AppHandle,
    state: State<'_, AppState>,
    theme: Theme,
) -> Result<AppSettings, String> {
    let mut settings = state.settings();
    settings.theme = theme;
    state.db.save_settings(&settings)?;
    state.replace_settings(settings.clone());
    windows::apply_theme(&app);
    Ok(settings)
}

#[tauri::command]
pub fn notify_avatar_state(app: AppHandle, state_name: String) {
    let allowed = [
        "idle",
        "warming",
        "listening",
        "thinking",
        "answering",
        "error",
    ];
    if allowed.contains(&state_name.as_str()) {
        let _ = app.emit("avatar://state", state_name);
    }
}
