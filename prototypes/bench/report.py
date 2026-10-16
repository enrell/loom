"""Renders bench/results/latest.json into prototypes/COMPARISON.md."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = json.loads((ROOT / "bench/results/latest.json").read_text())
SHELLS = [s for s in ("electron", "tauri", "egui", "egui-glow") if s in DATA]
NAMES = {"electron": "Electron", "tauri": "Tauri", "egui": "egui (wgpu)", "egui-glow": "egui (glow)"}


def get(shell, *path):
    v = DATA.get(shell, {})
    for k in path:
        if not isinstance(v, dict) or v.get(k) is None:
            return None
        v = v[k]
    return v


def row(label, values, fmt, best=min):
    """Table row; the best numeric value is bold."""
    nums = [v for v in values if isinstance(v, (int, float))]
    winner = best(nums) if len(nums) > 1 else None
    cells = []
    for v in values:
        if v is None:
            cells.append("—")
        else:
            text = fmt(v)
            cells.append(f"**{text}**" if v == winner else text)
    return f"| {label} | " + " | ".join(cells) + " |"


def table(title, rows):
    head = "| | " + " | ".join(NAMES[s] for s in SHELLS) + " |"
    sep = "|---|" + "---:|" * len(SHELLS)
    return "\n".join([f"### {title}", "", head, sep, *rows, ""])


def secs(v):
    return f"{v:.1f} s" if v < 60 else f"{int(v // 60)} min {v % 60:.0f} s"


def mb(v):
    return f"{v:,.0f} MB" if v >= 10 else f"{v:.1f} MB"


def pct(v):
    return f"{v:.1f} %"


def ms(v):
    return f"{v:,.0f} ms"


def build_rows(kind):
    return [
        row("Wall time", [get(s, "build", kind, "wall_s") for s in SHELLS], secs),
        row("CPU time (all processes)", [get(s, "build", kind, "cpu_s") for s in SHELLS], secs),
        row("Peak memory (job commit)", [get(s, "build", kind, "peak_commit_mb") for s in SHELLS], mb),
        row("Peak working set (sampled)", [get(s, "build", kind, "peak_working_set_mb") for s in SHELLS], mb),
    ]


def scenario_rows(name):
    return [
        row("CPU (% of one core)", [get(s, "runtime", name, "cpu_percent_mean") for s in SHELLS], pct),
        row("Private memory", [get(s, "runtime", name, "private_mb_mean") for s in SHELLS], mb),
        row("Working set", [get(s, "runtime", name, "working_set_mb_mean") for s in SHELLS], mb),
        row("Processes", [get(s, "runtime", name, "processes") for s in SHELLS], str),
    ]


m = DATA.get("machine", {})
out = [
    "# Loom UI shells — Electron vs Tauri vs egui",
    "",
    "**Outcome: Tauri 2 on a Rust core — see [DECISION.md](DECISION.md).**",
    "",
    "Three prototypes of the same Loom screen (mod list with 2,000 mods, downloads/nxm, embedded",
    "Nexus Mods browser, media, live performance). Electron and Tauri run the very same",
    "`ui/` (HTML/CSS/JS); egui redraws it natively from the same design tokens and data",
    "generator. `egui (glow)` is the same egui app with the OpenGL renderer instead of wgpu.",
    "",
    f"Measured {DATA.get('generated', '?')} on {m.get('cpu', '?')}, {m.get('logical_cpus', '?')} logical CPUs,",
    f"{m.get('ram_gb', '?')} GB RAM, {m.get('os', '?')}. Rust builds without sccache, `rust-lld` linker.",
    "Every number covers the whole process tree (Windows Job Object): rustc/cargo/node during",
    "builds; renderer, GPU and WebView2 helper processes at runtime.",
    "",
    "Conditions: the test windows ran on the laptop's built-in screen (1920×1200, 100 %, Intel Iris Xe)",
    "while the main screen was in use, so CPU numbers carry some background noise. egui uses the",
    "low-power (integrated) GPU, like Chromium/WebView2 do by default.",
    "",
    "## Screenshots",
    "",
    "| Electron | Tauri | egui |",
    "|---|---|---|",
]
for page in ("mods", "downloads", "nexus", "media", "perf"):
    out.append("| " + " | ".join(f"![{s} {page}](docs/shots/{s}-{page}.png)" for s in ("electron", "tauri", "egui")) + " |")
out += [
    "",
    "## Features",
    "",
    "| | Electron | Tauri | egui |",
    "|---|---|---|---|",
    "| Same UI code | `ui/` as is | `ui/` as is | redrawn in Rust (same tokens, data, layout) |",
    "| Embedded browser | ✅ own Chromium (`WebContentsView`) | ✅ WebView2 child webview (`unstable` feature) | ❌ none — opens the system browser |",
    "| Nexus Mods + login page + Cloudflare check | ✅ | ✅ | — |",
    "| `nxm://` from Windows (second instance) | ✅ single-instance lock | ✅ single-instance plugin | ✅ (hand-rolled local socket) |",
    "| `nxm://` clicked inside the embedded browser | ✅ caught before Windows | ✅ caught before Windows | — |",
    "| Local H.264/AAC video | ✅ | ✅ | ❌ needs ffmpeg/GStreamer bindings |",
    "| Calling openvfs (Rust) | via a native addon (napi-rs) or a helper process | direct, same process | direct, same process |",
    "| Linux | same Chromium everywhere | WebKitGTK (different engine, weaker video/codecs) | same code |",
    "",
    "Login itself (Nexus account, Google SSO) was not exercised: it needs real credentials.",
    "",
    "## Build",
    "",
    table("Clean release build", build_rows("clean_release")),
    table("Incremental release build (one source file touched)", build_rows("incremental_release")),
    table("Clean debug build", build_rows("clean_debug")),
    table("Incremental debug build (the dev loop)", build_rows("incremental_debug")),
    "Electron has no compile step: its \"build\" is copying the UI and packaging (asar + runtime).",
    "",
    table("Artifact", [row("Size on disk", [get(s, "build", "artifact_mb") for s in SHELLS], mb)]),
    "Tauri also needs the WebView2 runtime (preinstalled on Windows 10/11, ~shared with Edge).",
    "",
    "## Runtime",
    "",
    table("Startup (launch → first frame painted)", [
        row("Median", [get(s, "runtime", "startup", "external_ms_median") for s in SHELLS], ms),
        row("Best", [get(s, "runtime", "startup", "external_ms_min") for s in SHELLS], ms),
    ]),
    table("Idle on the mod list (20 s)", scenario_rows("idle")),
    table("Scrolling the 2,000-mod list continuously (15 s)", scenario_rows("scroll")),
    table("Playing the local video (15 s)", scenario_rows("video")),
    table("Nexus Mods page open in the embedded browser (15 s, after 15 s to load)", scenario_rows("nexus")),
    "CPU is the job's total CPU time over the window divided by wall time (100 % = one full",
    "core). Working set double-counts pages shared between processes; private memory does not.",
    "",
    "## How to reproduce",
    "",
    "```",
    "python bench/run_all.py      # builds, runtime, screenshots, nxm tests, this report",
    "```",
    "",
]
(ROOT / "COMPARISON.md").write_text("\n".join(out), encoding="utf-8")
print(f"wrote {ROOT / 'COMPARISON.md'}")
