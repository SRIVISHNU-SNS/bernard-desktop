# llama-server sidecar

Bernard runs inference through a private `llama-server.exe` process. Two builds are
supported so the app can fall back cleanly when a GPU driver refuses Vulkan:

- `sidecar/llama-vulkan/llama-server.exe` — Vulkan build from llama.cpp `b11889`
- `sidecar/llama-cpu/llama-server.exe` — CPU build from llama.cpp `b11889`

Both folders are copied into the installed resources directory by the Tauri bundle
configuration in `src-tauri/tauri.conf.json`. The app never downloads or executes a
server build at runtime; it only discovers the copies shipped beside the executable.

Use `scripts/fetch-sidecar.ps1` to populate the folders from the pinned upstream
release before packaging. Model weights are separate: they are downloaded on first run
from Hugging Face into `%APPDATA%\com.bernard.desktop\models`, or selected manually in
Settings.

The binaries come from <https://github.com/ggml-org/llama.cpp/releases/tag/b11889> and
remain under the upstream MIT license. The implementation and documentation in this
folder are part of Bernard.
