# Native desktop GUI with egui

Mosaic Maker is a single native desktop app built with eframe/egui. Generating and inspecting a Mosaic happen in the same window. The core interaction is a pan/zoom canvas with per-Tile hover and click hit-testing, and an immediate-mode GUI in pure Rust handles that with no second language or runtime. URLs can still be opened later for artist links.

## Considered Options

- **Tauri (Rust + web UI)**: more polished widgets, but adds a JS frontend and an IPC boundary right where the hot path is (hit-testing thousands of Tiles).
- **Rust → WASM in the browser**: browsers can't scan a local Source Library folder recursively without friction, and it caps memory for large libraries.
- **CLI generator + separate viewer**: inspection is the point of the app, so splitting it into two tools adds ceremony without benefit.
