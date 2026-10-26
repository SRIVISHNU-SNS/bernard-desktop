use crate::app_state::AppState;
use crate::models::{
    now_ms, BackendPreference, ModelFileInfo, ServerPhase, ServerStatus, DEFAULT_CONTEXT,
};
use crate::paths;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

const MAX_LOG_LINES: usize = 240;
const HEALTH_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct ChunkTiming {
    pub ttft_ms: Option<f64>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub cache_tokens: Option<i64>,
    pub tokens_per_second: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct StreamOutcome {
    pub content: String,
    pub timing: ChunkTiming,
    pub cancelled: bool,
}

struct Inner {
    status: ServerStatus,
    child: Option<Child>,
    logs: VecDeque<String>,
    base_url: Option<String>,
    generation: u64,
}

pub struct ServerManager {
    app: AppHandle,
    inner: Mutex<Inner>,
    client: reqwest::Client,
    restart: Notify,
    stop: CancellationToken,
    started: AtomicBool,
    shutting_down: AtomicBool,
    launch_generation: AtomicU64,
    job: Mutex<Option<isize>>,
}

impl ServerManager {
    pub fn new(app: AppHandle) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            // No total request timeout: prefill on a large model can run for
            // minutes before the first token. `read_timeout` only fires when the
            // server stops sending data entirely.
            .read_timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_default();
        Self {
            app,
            inner: Mutex::new(Inner {
                status: ServerStatus::stopped(),
                child: None,
                logs: VecDeque::new(),
                base_url: None,
                generation: 0,
            }),
            client,
            restart: Notify::new(),
            stop: CancellationToken::new(),
            started: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            launch_generation: AtomicU64::new(0),
            job: Mutex::new(None),
        }
    }

    pub async fn status(&self) -> ServerStatus {
        self.inner.lock().await.status.clone()
    }

    pub async fn base_url(&self) -> Option<String> {
        self.inner.lock().await.base_url.clone()
    }

    pub async fn logs(&self) -> Vec<String> {
        self.inner.lock().await.logs.iter().cloned().collect()
    }

    pub fn client(&self) -> reqwest::Client {
        self.client.clone()
    }

    pub async fn push_log(&self, line: impl Into<String>) {
        let mut inner = self.inner.lock().await;
        let line = line.into();
        tracing::debug!(target: "bernard::llama", "{line}");
        inner.logs.push_back(line);
        while inner.logs.len() > MAX_LOG_LINES {
            inner.logs.pop_front();
        }
    }

    async fn set_status(&self, status: ServerStatus) {
        {
            let mut inner = self.inner.lock().await;
            inner.status = status.clone();
            inner.base_url = status
                .port
                .map(|port| format!("http://127.0.0.1:{port}"));
        }
        let _ = self.app.emit("server://status", &status);
    }

    pub fn request_restart(&self) {
        self.restart.notify_waiters();
        self.launch_generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        self.stop.cancel();
        self.restart.notify_waiters();
        if let Some(state) = self.app.try_state::<AppState>() {
            let manager = state.server.clone();
            tauri::async_runtime::spawn(async move {
                manager.kill_child().await;
            });
        }
    }

    async fn kill_child(&self) {
        let mut inner = self.inner.lock().await;
        if let Some(child) = inner.child.as_mut() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        inner.child = None;
        inner.base_url = None;
    }

    pub async fn supervise(self: Arc<Self>) {
        let mut backoff_seconds = 2u64;
        loop {
            if self.shutting_down.load(Ordering::SeqCst) {
                break;
            }

            let settings = match self.app.try_state::<AppState>() {
                Some(state) => state.settings(),
                None => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };
            let model = ModelFileInfo::for_settings(&settings);
            if !model.ready() {
                let mut status = ServerStatus::stopped();
                status.phase = ServerPhase::ModelMissing;
                status.model_name = settings.model_preset.clone();
                status.message =
                    "The model files are not installed yet. Open Settings to download or choose them."
                        .to_string();
                self.set_status(status).await;
                tokio::select! {
                    _ = self.stop.cancelled() => break,
                    _ = self.restart.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                }
                continue;
            }

            let mut status = ServerStatus::stopped();
            status.phase = ServerPhase::Starting;
            status.model_name = settings.model_preset.clone();
            status.message = "Starting the local model server…".to_string();
            self.set_status(status).await;

            match self.launch(&settings).await {
                Ok(()) => {
                    backoff_seconds = 2;
                    self.wait_and_watch().await;
                }
                Err(error) => {
                    if self.shutting_down.load(Ordering::SeqCst) {
                        break;
                    }
                    let mut status = ServerStatus::stopped();
                    status.phase = ServerPhase::Error;
                    status.model_name = settings.model_preset.clone();
                    status.message = "Bernard could not start the local model.".to_string();
                    status.last_error = Some(error.clone());
                    self.set_status(status).await;
                    self.push_log(format!("startup failed: {error}")).await;
                }
            }

            if self.shutting_down.load(Ordering::SeqCst) {
                break;
            }
            self.kill_child().await;
            tokio::select! {
                _ = self.stop.cancelled() => break,
                _ = self.restart.notified() => {
                    backoff_seconds = 2;
                }
                _ = tokio::time::sleep(Duration::from_secs(backoff_seconds)) => {
                    backoff_seconds = (backoff_seconds * 2).min(30);
                }
            }
        }
        self.kill_child().await;
        let mut status = ServerStatus::stopped();
        status.message = "Stopped".to_string();
        self.set_status(status).await;
    }

    async fn launch(&self, settings: &crate::models::AppSettings) -> Result<(), String> {
        let candidates: Vec<(&str, u32)> = match settings.backend {
            BackendPreference::Cpu => vec![("llama-cpu", 0)],
            BackendPreference::Vulkan => vec![("llama-vulkan", 99)],
            BackendPreference::Auto => vec![("llama-vulkan", 99), ("llama-cpu", 0)],
        };

        let generation = self.launch_generation.load(Ordering::SeqCst);
        let mut failures = Vec::new();
        for (backend_dir, gpu_layers) in candidates {
            if self.shutting_down.load(Ordering::SeqCst)
                || self.launch_generation.load(Ordering::SeqCst) != generation
            {
                return Err("Startup was cancelled.".to_string());
            }
            let Some(binary) = paths::find_server_binary(&self.app, backend_dir) else {
                failures.push(format!("{backend_dir}: llama-server.exe is missing"));
                continue;
            };
            let label = if backend_dir.contains("vulkan") {
                "Vulkan GPU"
            } else {
                "CPU"
            };
            self.push_log(format!("starting {label} server: {}", binary.display()))
                .await;
            match self
                .spawn_and_wait(&binary, gpu_layers, label, settings, generation)
                .await
            {
                Ok(()) => return Ok(()),
                Err(error) => {
                    self.push_log(format!("{label} failed: {error}")).await;
                    failures.push(format!("{label}: {error}"));
                    self.kill_child().await;
                }
            }
        }

        Err(if failures.is_empty() {
            "No llama-server build was found. Run the setup step in the README.".to_string()
        } else {
            failures.join("\n")
        })
    }

    async fn spawn_and_wait(
        &self,
        binary: &std::path::Path,
        gpu_layers: u32,
        label: &str,
        settings: &crate::models::AppSettings,
        generation: u64,
    ) -> Result<(), String> {
        let port = free_port()?;
        let mut command = Command::new(binary);
        command
            .arg("--model")
            .arg(&settings.model_path)
            .arg("--mmproj")
            .arg(&settings.mmproj_path)
            .arg("--ctx-size")
            .arg(DEFAULT_CONTEXT.to_string())
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--no-webui")
            .arg("--parallel")
            .arg("1")
            .arg("--gpu-layers")
            .arg(gpu_layers.to_string())
            .arg("--flash-attn")
            .arg("on")
            .arg("--jinja")
            .arg("--log-colors")
            .arg("off")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(parent) = binary.parent() {
            command.current_dir(parent);
        }
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: the server is a background child of a GUI app.
            command.creation_flags(0x0800_0000);
        }

        let mut child = command
            .spawn()
            .map_err(|e| format!("Could not launch {}: {e}", binary.display()))?;

        #[cfg(windows)]
        {
            if let Some(raw) = child.raw_handle() {
                // `RawHandle` is a raw pointer and therefore not `Send`; pass it
                // as an integer so this future stays `Send` for `spawn`.
                self.attach_to_job(raw as usize).await;
            }
        }

        if let Some(stdout) = child.stdout.take() {
            let manager = self.self_ref();
            tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    manager.push_log(line).await;
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let manager = self.self_ref();
            tauri::async_runtime::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    manager.push_log(line).await;
                }
            });
        }

        let mut status = ServerStatus::stopped();
        status.phase = ServerPhase::Warming;
        status.backend = Some(label.to_string());
        status.port = Some(port);
        status.model_name = settings.model_preset.clone();
        status.message = format!("Loading the model on {label}…");
        status.started_at = Some(now_ms());
        self.set_status(status.clone()).await;

        let started = Instant::now();
        let base = format!("http://127.0.0.1:{port}");
        loop {
            if self.shutting_down.load(Ordering::SeqCst) {
                let _ = child.kill().await;
                return Err("Startup was cancelled.".to_string());
            }
            if self.launch_generation.load(Ordering::SeqCst) != generation {
                let _ = child.kill().await;
                return Err("Restart requested.".to_string());
            }
            match child.try_wait() {
                Ok(Some(exit)) => {
                    return Err(format!(
                        "llama-server exited with {exit} before it became ready"
                    ));
                }
                Ok(None) => {}
                Err(error) => return Err(format!("Could not read llama-server state: {error}")),
            }
            match self
                .client
                .get(format!("{base}/health"))
                .timeout(Duration::from_secs(3))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    {
                        let mut inner = self.inner.lock().await;
                        inner.child = Some(child);
                        inner.base_url = Some(base.clone());
                        inner.generation = inner.generation.wrapping_add(1);
                    }
                    let mut ready = status.clone();
                    ready.phase = ServerPhase::Ready;
                    ready.message = format!("Ready on {label}");
                    ready.last_error = None;
                    self.set_status(ready).await;
                    self.app.emit("avatar://state", "ready").ok();
                    return Ok(());
                }
                Ok(_) => {}
                Err(_) => {}
            }
            if started.elapsed() > HEALTH_TIMEOUT {
                let _ = child.kill().await;
                return Err("The model did not become ready within five minutes.".to_string());
            }
            tokio::select! {
                _ = self.stop.cancelled() => {
                    let _ = child.kill().await;
                    return Err("Startup was cancelled.".to_string());
                }
                _ = self.restart.notified() => {
                    let _ = child.kill().await;
                    return Err("Restart requested.".to_string());
                }
                _ = tokio::time::sleep(Duration::from_millis(350)) => {}
            }
        }
    }

    async fn wait_and_watch(&self) {
        loop {
            tokio::select! {
                _ = self.stop.cancelled() => return,
                _ = self.restart.notified() => return,
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
            let exited = {
                let mut inner = self.inner.lock().await;
                match inner.child.as_mut() {
                    Some(child) => match child.try_wait() {
                        Ok(Some(exit)) => Some(Ok(exit)),
                        Ok(None) => None,
                        Err(error) => Some(Err(error.to_string())),
                    },
                    None => Some(Ok(std::process::ExitStatus::default())),
                }
            };
            if let Some(result) = exited {
                let detail = match result {
                    Ok(exit) => format!("llama-server stopped ({exit})."),
                    Err(error) => format!("llama-server state failed: {error}"),
                };
                self.push_log(detail.clone()).await;
                let mut status = ServerStatus::stopped();
                status.phase = ServerPhase::Error;
                status.message = "The local model stopped unexpectedly.".to_string();
                status.last_error = Some(detail);
                let restarts = {
                    let inner = self.inner.lock().await;
                    inner.status.restarts + 1
                };
                status.restarts = restarts;
                self.set_status(status).await;
                self.kill_child().await;
                return;
            }
        }
    }

    fn self_ref(&self) -> Arc<ServerManager> {
        self.app.state::<AppState>().server.clone()
    }

    #[cfg(windows)]
    async fn attach_to_job(&self, raw: usize) {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        let mut job = self.job.lock().await;
        if job.is_none() {
            unsafe {
                if let Ok(handle) = CreateJobObjectW(None, None) {
                    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                    let configured = SetInformationJobObject(
                        handle,
                        JobObjectExtendedLimitInformation,
                        &info as *const _ as *const std::ffi::c_void,
                        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                    );
                    if configured.is_ok() {
                        *job = Some(handle.0 as isize);
                    }
                }
            }
        }
        if let Some(job_handle) = *job {
            unsafe {
                let process = HANDLE(raw as *mut std::ffi::c_void);
                let _ = AssignProcessToJobObject(HANDLE(job_handle as *mut std::ffi::c_void), process);
            }
        }
    }

    pub async fn chat_stream<F>(
        &self,
        body: Value,
        cancel: CancellationToken,
        mut on_delta: F,
    ) -> Result<StreamOutcome, String>
    where
        F: FnMut(&str),
    {
        let base = self
            .base_url()
            .await
            .ok_or_else(|| "The local model is not ready yet.".to_string())?;
        let response = self
            .client
            .post(format!("{base}/v1/chat/completions"))
            .json(&body)
            .send()
            .await
            .map_err(|e| friendly_network_error(&e))?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(server_error_message(status.as_u16(), &detail));
        }

        let started = Instant::now();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut first_token_at: Option<f64> = None;
        let mut timing = ChunkTiming {
            ttft_ms: None,
            prompt_tokens: None,
            completion_tokens: None,
            cache_tokens: None,
            tokens_per_second: None,
        };
        let mut finished = false;
        let mut cancelled = false;

        'outer: while let Some(chunk) = tokio::select! {
            _ = cancel.cancelled() => { cancelled = true; break 'outer; }
            next = stream.next() => next,
        } {
            let chunk = chunk.map_err(|e| friendly_network_error(&e))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(position) = buffer.find('\n') {
                let line = buffer[..position].trim().to_string();
                buffer.drain(..=position);
                let Some(payload) = line.strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim();
                if payload == "[DONE]" {
                    finished = true;
                    break;
                }
                let Ok(value) = serde_json::from_str::<Value>(payload) else {
                    continue;
                };
                if let Some(error) = value.get("error") {
                    return Err(error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("The model reported an error.")
                        .to_string());
                }
                if let Some(choice) = value
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|choices| choices.first())
                {
                    if let Some(delta) = choice
                        .get("delta")
                        .and_then(|delta| delta.get("content"))
                        .and_then(Value::as_str)
                    {
                        if !delta.is_empty() {
                            if first_token_at.is_none() {
                                first_token_at = Some(started.elapsed().as_secs_f64() * 1000.0);
                            }
                            content.push_str(delta);
                            on_delta(delta);
                        }
                    }
                }
                if let Some(timings) = value.get("timings") {
                    read_timings(&mut timing, timings);
                }
                if let Some(usage) = value.get("usage") {
                    timing.prompt_tokens = usage
                        .get("prompt_tokens")
                        .and_then(Value::as_i64)
                        .or(timing.prompt_tokens);
                    timing.completion_tokens = usage
                        .get("completion_tokens")
                        .and_then(Value::as_i64)
                        .or(timing.completion_tokens);
                }
                if value
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|choices| choices.first())
                    .and_then(|choice| choice.get("finish_reason"))
                    .and_then(Value::as_str)
                    .is_some()
                {
                    finished = true;
                }
            }
            if finished {
                break;
            }
        }

        timing.ttft_ms = first_token_at;
        if timing.tokens_per_second.is_none() {
            if let (Some(ms), Some(tokens)) = (timing.ttft_ms, timing.completion_tokens) {
                if tokens > 0 && ms > 0.0 {
                    timing.tokens_per_second = Some(tokens as f64 / (ms / 1000.0));
                }
            }
        }
        Ok(StreamOutcome {
            content,
            timing,
            cancelled,
        })
    }
}

fn read_timings(timing: &mut ChunkTiming, value: &Value) {
    timing.prompt_tokens = value
        .get("prompt_n")
        .and_then(Value::as_i64)
        .or(timing.prompt_tokens);
    timing.completion_tokens = value
        .get("predicted_n")
        .and_then(Value::as_i64)
        .or(timing.completion_tokens);
    timing.cache_tokens = value
        .get("cache_n")
        .and_then(Value::as_i64)
        .or(timing.cache_tokens);
    if let Some(predicted_ms) = value.get("predicted_ms").and_then(Value::as_f64) {
        if let Some(predicted_n) = value.get("predicted_n").and_then(Value::as_f64) {
            if predicted_ms > 0.0 {
                timing.tokens_per_second = Some(predicted_n / (predicted_ms / 1000.0));
            }
        }
    }
}

fn free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Could not reserve a local port: {e}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|e| format!("Could not read the local port: {e}"))
}

fn friendly_network_error(error: &reqwest::Error) -> String {
    if error.is_connect() {
        "The local model is not accepting connections. It may still be starting.".to_string()
    } else if error.is_timeout() {
        "The local model took too long to answer.".to_string()
    } else {
        format!("The local model connection failed: {error}")
    }
}

fn server_error_message(status: u16, detail: &str) -> String {
    let brief: String = detail.chars().take(600).collect();
    match status {
        400 => format!("The model rejected this request: {brief}"),
        413 => "This conversation is larger than the model's context window.".to_string(),
        500..=599 if brief.to_lowercase().contains("memory") => {
            "The model ran out of memory. Try the Faster model in Settings.".to_string()
        }
        500..=599 => format!("The local model failed: {brief}"),
        _ => format!("The local model returned HTTP {status}: {brief}"),
    }
}

pub fn start(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.server.started.swap(true, Ordering::SeqCst) {
        return;
    }
    let manager = state.server.clone();
    tauri::async_runtime::spawn(async move {
        manager.supervise().await;
    });
}

pub fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.server.shutdown();
    }
}

pub fn request_restart(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.server.request_restart();
    }
}

pub fn build_chat_body(
    messages: Vec<Value>,
    max_tokens: u32,
    temperature: f32,
) -> Value {
    json!({
        "model": "gemma",
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
        "cache_prompt": true,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "top_p": 0.95,
        "top_k": 40,
        "repeat_penalty": 1.05
    })
}

#[cfg(windows)]
pub fn set_capture_exclusion(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
    };
    if let Ok(handle) = window.hwnd() {
        // Tauri bundles a newer `windows` crate than our direct dependency, so
        // rebuild the handle from its raw pointer instead of mixing crate types.
        let handle = HWND(handle.0);
        unsafe {
            let _ = SetWindowDisplayAffinity(handle, WDA_EXCLUDEFROMCAPTURE);
        }
    }
}
