# 🤖 Bernard: Local AI Desktop Companion

Bernard is a production-grade, fully local AI avatar for Windows. He lives as a transparent, draggable entity on your desktop and provides an intelligent interface for chatting and screen-context analysis using Gemma 4 via `llama.cpp`.

## 🚀 Key Features
- **100% Local Inference**: No data ever leaves your machine. Bernard uses Gemma 4 (GGUF) bundled via a hidden `llama-server`.
- **Intelligent Screen Snipping**: 
  - Global hotkey (`Alt+B` by default) to capture any monitor.
  - Interactive overlay to crop and downscale images.
  - Automatic attachment to chat with prompt-cache preservation.
- **Windows-Native UX**:
  - 72x72 transparent avatar with state-based animations.
  - 420x600 docked panel with markdown support, code highlighting, and tables.
  - Mixed-DPI correct layout and multi-monitor support.
- **Performance Optimized**: 
  - Vulkan acceleration with CPU fallback.
  - Prompt caching for near-instant follow-up responses.
  - Pre-warmed overlay windows for <150ms hotkey latency.

## 🛠️ Installation & Setup

### Prerequisites
- **Windows 10/11**
- **GPU (Optional)**: NVIDIA/AMD/Intel GPU with Vulkan support is highly recommended for faster inference.

### Installation
1. Download the `Bernard_Setup.exe` from the releases.
2. Run the installer.
3. **Important**: Because the installer is unsigned, Windows Smart App Control may block it. If you receive a "blocked by policy" error:
   - Open PowerShell as Administrator.
   - Run: `Set-ItemProperty -Path "HKLM:\\SYSTEM\\CurrentControlSet\\Control\\CI\\Policy" -Name "VerifiedAndReputablePolicyState" -Value 0`
   - Reboot your computer.

### Model Provisioning
On first launch, Bernard will guide you to download a model preset:
- **Faster (E2B)**: Quick responses, lower RAM usage.
- **Balanced (E4B)**: The sweet spot for most tasks.
- **Smarter (12B)**: Highest reasoning capability, requires more VRAM/RAM.

## 📊 Measured Performance
*Measurements taken on: Windows 10, 16GB RAM, [INSERT GPU]*

| Metric | E2B (Faster) | E4B (Balanced) | 12B (Smarter) |
|---|---|---|---|
| **TTFT (Time to First Token)** | [TBD] ms | [TBD] ms | [TBD] ms |
| **Tokens per Second** | [TBD] t/s | [TBD] t/s | [TBD] t/s |
| **Peak RAM Usage** | [TBD] GB | [TBD] GB | [TBD] GB |
| **Hotkey Latency** | < 150ms | < 150ms | < 150ms |

## ⚙️ Configuration
Accessible via the settings menu:
- **Hotkey**: Change the global snip trigger (Default: `Alt+B`).
- **Theme**: Light, Dark, or System.
- **Avatar Size**: Adjust the visual size of Bernard on your desktop.
- **Model Paths**: Manually point to GGUF files if you have custom models.

## 🔒 Privacy & Security
- **No Telemetry**: Bernard contains zero tracking or phone-home logic.
- **Local Storage**: All conversations and snips are stored in `%APPDATA%/com.bernard.desktop` using an encrypted SQLite database.
- **Strict CSP**: The frontend is locked down with a minimal Content Security Policy.
