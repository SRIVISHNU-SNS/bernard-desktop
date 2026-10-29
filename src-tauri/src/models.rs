use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const APP_ID: &str = "com.bernard.desktop";
pub const SERVER_BUILD: &str = "b11889";
pub const DEFAULT_CONTEXT: u32 = 8192;
pub const SNIP_MAX_SIDE: u32 = 1568;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BackendPreference {
    Auto,
    Vulkan,
    Cpu,
}

impl Default for BackendPreference {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub api_key: String,
    pub model_preset: String,
    pub hotkey: String,
    pub theme: Theme,
    pub avatar_size: u32,
    pub start_with_windows: bool,
    pub system_prompt: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub backend: BackendPreference,
    pub show_performance: bool,
    #[serde(default)]
    pub avatar_x: Option<i32>,
    #[serde(default)]
    pub avatar_y: Option<i32>,
}

impl AppSettings {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            api_key: "".to_string(),
            model_preset: "gemini-1.5-flash".to_string(),
            hotkey: "Alt+B".to_string(),
            theme: Theme::System,
            avatar_size: 72,
            start_with_windows: false,
            system_prompt: default_system_prompt(),
            max_tokens: 768,
            temperature: 0.55,
            backend: BackendPreference::Auto,
            show_performance: true,
            avatar_x: None,
            avatar_y: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(56..=96).contains(&self.avatar_size) {
            return Err("Avatar size must be between 56 and 96.".to_string());
        }
        if !(64..=4096).contains(&self.max_tokens) {
            return Err("Max tokens must be between 64 and 4096.".to_string());
        }
        if !(0.0..=2.0).contains(&self.temperature) {
            return Err("Temperature must be between 0 and 2.".to_string());
        }
        if self.system_prompt.trim().is_empty() {
            return Err("The system prompt cannot be empty.".to_string());
        }
        if self.hotkey.trim().is_empty() {
            return Err("The hotkey cannot be empty.".to_string());
        }
        Ok(())
    }
}

pub fn default_system_prompt() -> String {
    "You are Bernard, a concise desktop assistant. Be direct, warm, and honest about uncertainty. \
Answer the question that was asked. When explaining a doubt, use a short numbered sequence. \
For code, give complete runnable answers and mention pitfalls only when they matter. \
If an image is supplied, inspect it carefully and do not invent details that are not visible."
        .to_string()
}

#[derive(Debug, Clone)]
pub struct ModelPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub note: &'static str,
    pub repo: &'static str,
    pub model_file: &'static str,
    pub mmproj_file: &'static str,
    pub model_bytes: u64,
    pub mmproj_bytes: u64,
    pub model_sha256: &'static str,
    pub mmproj_sha256: &'static str,
}

pub const PRESETS: [ModelPreset; 3] = [
    ModelPreset {
        id: "e2b",
        label: "Faster",
        note: "Gemma 4 E2B · Q4_0 · quickest replies, lighter memory",
        repo: "ggml-org/gemma-4-E2B-it-GGUF",
        model_file: "gemma-4-E2B-it-Q4_0.gguf",
        mmproj_file: "mmproj-gemma-4-E2B-it-Q8_0.gguf",
        model_bytes: 2_841_481_184,
        mmproj_bytes: 557_368_064,
        model_sha256: "8e30dff3ac4c8434c49a7036fa15564bdbb6044e42bf04550bf1a096ad7e6a52",
        mmproj_sha256: "9406f99c16d68cda4f1f0552192dcc99021ea1fc6d2fd50b1dc3ccf30d04b292",
    },
    ModelPreset {
        id: "e4b",
        label: "Balanced",
        note: "Gemma 4 E4B · Q4_0 · recommended for this machine",
        repo: "ggml-org/gemma-4-E4B-it-GGUF",
        model_file: "gemma-4-E4B-it-Q4_0.gguf",
        mmproj_file: "mmproj-gemma-4-E4B-it-Q8_0.gguf",
        model_bytes: 4_590_807_392,
        mmproj_bytes: 559_874_816,
        model_sha256: "a555b900214b477d8880e7832e0b8925e139b0159640036b09fe472b6f2097f2",
        mmproj_sha256: "197f49a93027f9843772bd24a6a9e0be2a32a788de5a3def330e9c585d86edd1",
    },
    ModelPreset {
        id: "12b",
        label: "Smarter",
        note: "Gemma 4 12B · Q4_0 · slower, needs about 9 GB free RAM",
        repo: "ggml-org/gemma-4-12B-it-GGUF",
        model_file: "gemma-4-12B-it-Q4_0.gguf",
        mmproj_file: "mmproj-gemma-4-12B-it-Q8_0.gguf",
        model_bytes: 7_219_673_216,
        mmproj_bytes: 158_987_616,
        model_sha256: "3712b9bd32cae83a22f67ee7a4466d8d7a4f21646ac8a07d19bf9418e8767a70",
        mmproj_sha256: "59e62255435dda870e2d1de97cc031330b31a898bac12b38a182cecff9cd3738",
    },
];

pub fn preset(id: &str) -> &'static ModelPreset {
    PRESETS
        .iter()
        .find(|p| p.id == id)
        .unwrap_or(&PRESETS[1])
}

pub fn preset_model_url(preset: &ModelPreset) -> String {
    format!(
        "https://huggingface.co/{}/resolve/main/{}?download=true",
        preset.repo, preset.model_file
    )
}

pub fn preset_mmproj_url(preset: &ModelPreset) -> String {
    format!(
        "https://huggingface.co/{}/resolve/main/{}?download=true",
        preset.repo, preset.mmproj_file
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub settings: AppSettings,
    pub server: ServerStatus,
    pub model: ModelFileInfo,
    pub conversations: Vec<Conversation>,
    pub app_version: String,
    pub data_dir: String,
    pub started_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFileInfo {
    pub preset_id: String,
    pub model_path: String,
    pub mmproj_path: String,
    pub model_present: bool,
    pub mmproj_present: bool,
    pub model_bytes: u64,
    pub mmproj_bytes: u64,
}

impl ModelFileInfo {
    pub fn for_settings(settings: &AppSettings) -> Self {
        let model = PathBuf::from(&settings.model_path);
        let mmproj = PathBuf::from(&settings.mmproj_path);
        Self {
            preset_id: settings.model_preset.clone(),
            model_path: settings.model_path.clone(),
            mmproj_path: settings.mmproj_path.clone(),
            model_present: model.is_file(),
            mmproj_present: mmproj.is_file(),
            model_bytes: std::fs::metadata(&model).map(|m| m.len()).unwrap_or(0),
            mmproj_bytes: std::fs::metadata(&mmproj).map(|m| m.len()).unwrap_or(0),
        }
    }

    pub fn ready(&self) -> bool {
        self.model_present && self.mmproj_present
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerPhase {
    Stopped,
    ModelMissing,
    Starting,
    Warming,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub phase: ServerPhase,
    pub backend: Option<String>,
    pub message: String,
    pub port: Option<u16>,
    pub model_name: String,
    pub restarts: u32,
    pub started_at: Option<i64>,
    pub last_error: Option<String>,
}

impl ServerStatus {
    pub fn stopped() -> Self {
        Self {
            phase: ServerPhase::Stopped,
            backend: None,
            message: "Not running".to_string(),
            port: None,
            model_name: String::new(),
            restarts: 0,
            started_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub image_paths: Vec<String>,
    pub created_at: i64,
    pub ttft_ms: Option<i64>,
    pub tokens_per_second: Option<f64>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub cache_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub path: String,
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnipReady {
    pub attachment: Attachment,
    pub source: String,
    pub panel_was_visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub conversation_id: Option<String>,
    pub content: String,
    pub image_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageResult {
    pub conversation_id: String,
    pub user_message: StoredMessage,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatDelta {
    pub request_id: String,
    pub conversation_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatDone {
    pub request_id: String,
    pub conversation_id: String,
    pub message: StoredMessage,
    pub context_trimmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFailed {
    pub request_id: String,
    pub conversation_id: String,
    pub message: String,
    pub partial: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSample {
    pub id: i64,
    pub kind: String,
    pub value_ms: f64,
    pub detail: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub server: ServerStatus,
    pub model: ModelFileInfo,
    pub app_version: String,
    pub llama_build: String,
    pub os: String,
    pub cpu: String,
    pub memory_total_bytes: u64,
    pub process_memory_bytes: u64,
    pub process_cpu_percent: f64,
    pub recent_logs: Vec<String>,
    pub performance: Vec<PerformanceSample>,
    pub last_tokens_per_second: Option<f64>,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub preset_id: String,
    pub file: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: f64,
    pub speed_bytes_per_second: u64,
    pub error: Option<String>,
    pub done: bool,
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
