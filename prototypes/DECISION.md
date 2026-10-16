# Decision: Tauri 2, on top of a Rust core

**Loom's desktop app will be a Tauri 2 shell (WebView2 on Windows) with the UI in web tech, sitting
on a Rust core library that also powers the CLI, the MCP server and the SDK.**

Data: [COMPARISON.md](COMPARISON.md) (same UI in all three shells, measured on 2026-10-16).

## What decided it

| Requirement | Electron | Tauri | egui |
|---|---|---|---|
| Embedded Nexus browser, login page, Cloudflare check | ✅ | ✅ | ❌ no browser |
| `nxm://` from Windows **and** from inside the embedded browser | ✅ | ✅ | ⚠️ Windows only |
| Video (mod showcases) | ✅ | ✅ | ❌ |
| Download size for an open-source release | ❌ 368 MB unpacked (~100 MB installer, every update) | ✅ **10 MB** (WebView2 ships with Windows) | ✅ 8–14 MB |
| openvfs / core in Rust | ⚠️ native addon or helper process | ✅ same process | ✅ same process |
| Startup | ✅ 269 ms | ⚠️ 514 ms | ✅ 129–300 ms |
| Idle memory (private) | 171 MB | 238 MB | 54–290 MB |
| Embedded Nexus page memory | 909 MB | 943 MB | — |
| Dev loop (incremental debug build) | instant (no compile) | 6.8 s | 3.4–4.2 s |

1. **egui is out.** It was the lightest by far (glow: 129 ms start, 54 MB), but it has no browser
   and no video, and the integrated Nexus experience is a hard requirement. Reproducing it would mean
   embedding a webview anyway — which is what Tauri already is.
2. **Electron vs Tauri on Windows is a tie at runtime.** WebView2 *is* Chromium: both load Nexus,
   pass Cloudflare, play H.264, catch `nxm://`, and cost the same once the Nexus page is open
   (~0.9 GB, the page itself dominates). Electron started faster and idled ~70 MB lower; Tauri
   was slightly lighter with video and Nexus. None of these gaps is one a user would feel.
3. **Distribution breaks the tie.** For an open-source project that strangers download and update,
   a 10 MB app versus a ~100 MB installer (368 MB on disk) per release is the difference users
   *do* see — and it is exactly where Vortex (Electron) is criticized.
4. **Loom's heart is Rust.** openvfs, the deployer, the Nexus API client, the CLI, the MCP server
   and the SDK are one Rust library. Tauri links it in-process with typed commands; Electron would
   need a napi-rs addon or a sidecar process plus a second build toolchain for contributors.
5. **Contributors still write web UI.** The frontend stays HTML/CSS/TypeScript, the most common
   skill set for open-source UI work — Tauri's Rust part is the core contributors touch anyway.

## Known costs, and what we do about them

- **Startup 514 ms** — WebView2 initialization. Show the window shell immediately and keep the
  first screen light; measured again once the real UI exists.
- **Embedded browser uses Tauri's `unstable` multi-webview API.** If it breaks on an upgrade, the
  fallback is a separate stable `WebviewWindow` for Nexus (same behavior, own window).
- **Linux** — Tauri uses WebKitGTK there (weaker codecs, possibly Cloudflare friction). openvfs is
  Windows-only today, so Linux is not v1; when it is, the `nxm://` + system-browser path works on any
  engine and the embedded browser can be optional.
- **Clean builds are heavy (~2 min, ~4 GB RAM)** — normal for Rust; CI caches them, and the
  incremental loop is seconds.

## What this means for the repository

```
loom/
  crates/
    loom-core/     # profiles, mod library, downloads, Nexus API, conflicts, openvfs integration
    loom-cli/      # `loom ...`, JSON output for agents
    loom-mcp/      # MCP server over loom-core
    loom-sdk/      # plugin API (stable, versioned)
  apps/
    desktop/       # Tauri 2 shell + web UI (TypeScript)
  prototypes/      # this comparison, kept for reference
```

The prototypes' `ui/` design (tokens, layout, components) is the starting point for `apps/desktop`.
