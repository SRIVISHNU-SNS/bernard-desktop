use crate::app_state::AppState;
use crate::models::{now_ms, ServerPhase, ServerStatus};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const MAX_LOG_LINES: usize = 240;

#[derive(Debug, Clone, Default)]
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
    logs: VecDeque<String>,
}

pub struct ServerManager {
    app: AppHandle,
    inner: Mutex<Inner>,
    client: reqwest::Client,
    stop: CancellationToken,
    // We no longer need JobObjects or Child processes because we use the Gemini API.
}

impl ServerManager {
    pub fn new(app: AppHandle) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            app,
            inner: Mutex::new(Inner {
                status: ServerStatus::stopped(),
                logs: VecDeque::new(),
            }),
            client,
            stop: CancellationToken::new(),
        }
    }

    pub async fn status(&self) -> ServerStatus {
        self.inner.lock().await.status.clone()
    }

    pub async fn logs(&self) -> Vec<String> {
        self.inner.lock().await.logs.iter().cloned().collect()
    }

    pub async fn push_log(&self, line: impl Into<String>) {
        let mut inner = self.inner.lock().await;
        let line = line.into();
        inner.logs.push_back(line);
        while inner.logs.len() > MAX_LOG_LINES {
            inner.logs.pop_front();
        }
    }

    async fn set_status(&self, status: ServerStatus) {
        let mut inner = self.inner.lock().await;
        inner.status = status;
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
        let state = self.app.state::<AppState>();
        let settings = state.settings();
        let api_key = settings.api_key.clone();
        
        if api_key.is_empty() {
            return Err("Gemini API key is missing. Please add it in settings.".to_string());
        }

        let model = &settings.model_preset;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse&key={}",
            api_key
        );

        let response = tokio::select! {
            _ = cancel.cancelled() => {
                return Ok(StreamOutcome {
                    content: String::new(),
                    timing: ChunkTiming::default(),
                    cancelled: true,
                });
            }
            res = self.client.post(&url).json(&body).send().await => res
        }.map_err(|e| format!(\"Network error: {e}\"))?;

        if !response.status().is_success() {
            let err_text = response.text().await.unwrap_or_default();
            return Err(format!(\"API Error: {err_text}\"));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut first_token_at: Option<f64> = None;
        let started = Instant::now();
        let mut timing = ChunkTiming::default();
        let mut finished = false;
        let mut cancelled = false;

        'outer: while let Some(chunk) = tokio::select! {
            _ = cancel.cancelled() => { cancelled = true; break 'outer; }
            next = stream.next() => next,
        } {
            let chunk = chunk.map_err(|e| format!(\"Stream error: {e}\"))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            
            while let Some(pos) = buffer.find('\\n') {
                let line = buffer[..pos].trim().to_string();
                buffer.drain(..=pos);
                
                if line.starts_with(\"data: \") {
                    let payload = &line[6..];
                    if payload == \"[DONE]\" {
                        finished = true;
                        break;
                    }
                    
                    if let Ok(value) = serde_json::from_str::<Value>(payload) {
                        if let Some(candidates) = value.get(\"candidates\").and_then(|c| c.as_array()) {
                            if let Some(candidate) = candidates.first() {
                                if let Some(content_part) = candidate.get(\"content\").and_then(|c| c.get(\"parts\")).and_then(|p| p.as_array()).and_then(|p| p.first()) {
                                    if let Some(text) = content_part.get(\"text\").and_then(|t| t.as_str()) {
                                        if !text.is_empty() {
                                            if first_token_at.is_none() {
                                                first_token_at = Some(started.elapsed().as_secs_f64() * 1000.0);
                                            }
                                            content.push_str(text);
                                            on_delta(text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if finished { break; }
        }

        timing.ttft_ms = first_token_at;
        Ok(StreamOutcome { content, timing, cancelled })
    }
}
