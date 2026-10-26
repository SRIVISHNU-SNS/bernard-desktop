# Bernard

Desktop app for screen capture + local AI inference on Windows (Tauri 2 + Rust + llama-server sidecar).

## Architecture
- Rust shell (Tauri 2) + WebView2 card
- llama-server sidecar (Vulkan, GGUF + mmproj)
- Win32 hotkey, Windows Graphics Capture/DXGI, native overlay
- Local-only, 127.0.0.1 + per-launch bearer token
- Gemma 4 E2B/E4B, reasoning off by default
- Shared RAM KV cache, single slot, 4-8K context

See full architecture in design doc.
