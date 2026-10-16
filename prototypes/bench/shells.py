"""Where each prototype lives and how to build/run it — shared by snap.py and bench.py."""
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent  # prototypes/


def electron_exe():
    packaged = ROOT / "electron/out/loom-electron-win32-x64/loom-electron.exe"
    if packaged.exists():
        return [str(packaged)]
    return [str(ROOT / "electron/node_modules/electron/dist/electron.exe"), str(ROOT / "electron")]


SHELLS = {
    "electron": {
        "run": electron_exe,
        "cwd": ROOT / "electron",
        "clean": [ROOT / "electron/out", ROOT / "electron/ui"],
        # Electron has no compile step: "build" = copy UI + package with asar.
        "build": ["npm", "run", "package"],
        "touch": ROOT / "electron/main.js",
        "artifact": ROOT / "electron/out/loom-electron-win32-x64",
    },
    "tauri": {
        "run": lambda: [str(ROOT / "tauri/src-tauri/target/release/loom-tauri.exe")],
        "cwd": ROOT / "tauri",
        "clean": [ROOT / "tauri/src-tauri/target"],
        "build": ["npm", "run", "build"],
        "debug": (ROOT / "tauri/src-tauri", ["cargo", "build"]),
        "touch": ROOT / "tauri/src-tauri/src/main.rs",
        "artifact": ROOT / "tauri/src-tauri/target/release/loom-tauri.exe",
    },
    "egui": {
        "run": lambda: [str(ROOT / "egui/target/release/loom-egui.exe")],
        "cwd": ROOT / "egui",
        "clean": [ROOT / "egui/target"],
        "build": ["cargo", "build", "--release"],
        "debug": (ROOT / "egui", ["cargo", "build"]),
        "touch": ROOT / "egui/src/main.rs",
        "artifact": ROOT / "egui/target/release/loom-egui.exe",
    },
    # Same egui app, OpenGL renderer instead of wgpu/DirectX 12.
    "egui-glow": {
        "run": lambda: [str(ROOT / "egui/target-glow/release/loom-egui.exe")],
        "cwd": ROOT / "egui",
        "clean": [ROOT / "egui/target-glow"],
        "build": ["cargo", "build", "--release", "--no-default-features", "--features", "glow", "--target-dir", "target-glow"],
        "debug": (ROOT / "egui", ["cargo", "build", "--no-default-features", "--features", "glow", "--target-dir", "target-glow"]),
        "touch": ROOT / "egui/src/main.rs",
        "artifact": ROOT / "egui/target-glow/release/loom-egui.exe",
    },
}


# Where test windows open (top-left, physical px): the built-in laptop
# screen, left of the main one. Override with LOOM_BENCH_WINDOW=x,y.
BENCH_WINDOW = os.environ.get("LOOM_BENCH_WINDOW", "-1880,-17")


def run_env(ready_file=None, scenario=None):
    env = dict(os.environ)
    env.pop("ELECTRON_RUN_AS_NODE", None)
    env["LOOM_BENCH_WINDOW"] = BENCH_WINDOW
    if ready_file:
        env["LOOM_BENCH_READY_FILE"] = str(ready_file)
    if scenario:
        env["LOOM_BENCH_SCENARIO"] = scenario
    return env


def build_env():
    env = dict(os.environ)
    # sccache is configured globally on this machine; it would turn clean
    # Rust builds into cache hits. Every build here is a real compile.
    env["RUSTC_WRAPPER"] = ""
    env["CARGO_INCREMENTAL"] = env.get("CARGO_INCREMENTAL", "")
    return env
