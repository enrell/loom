# Loom UI shell prototypes

The same Loom screen built three ways, to choose the app's shell:

| Folder | Shell | UI |
|---|---|---|
| `electron/` | Electron 44 | `ui/` (shared HTML/CSS/JS) |
| `tauri/` | Tauri 2 (WebView2) | `ui/` (shared HTML/CSS/JS) |
| `egui/` | egui 0.37 (wgpu or glow) | redrawn natively from the same tokens and data |
| `host-rs/` | — | metrics / startup / nxm helpers shared by the two Rust shells |
| `bench/` | — | benchmark harness, screenshots, nxm tests, report |

Each screen: a 2,000-mod list (virtualized, filter, search, toggles, details), Downloads
(`nxm://` links received from Windows or from the embedded browser), an embedded Nexus Mods
browser, local video + codec support, and live CPU/memory of the whole process tree.

## Setup

```powershell
# sample video for the Media page (Big Buck Bunny, CC BY 3.0, Blender Foundation)
curl -L -o ui/media/sample.mp4 https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/720/Big_Buck_Bunny_720_10s_1MB.mp4
cd electron; npm install; cd ..
cd tauri;    npm install; cd ..
pip install psutil pillow
```

## Run one

```powershell
cd electron; npm start                 # Electron
cd tauri;    npm run dev               # Tauri
cd egui;     cargo run --release       # egui (wgpu); --no-default-features --features glow for OpenGL
```

## Compare

```powershell
python bench/run_all.py   # ~25 min, opens windows: run it on an idle machine
```

Writes `bench/results/latest.json`, screenshots in `docs/shots/`, and `COMPARISON.md`.
