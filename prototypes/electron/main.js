// Loom UI shell prototype — Electron. Hosts ui/ (shared with the Tauri
// shell) and implements the host API from preload.js.
const { app, BrowserWindow, WebContentsView, ipcMain, shell, Menu } = require("electron");
const path = require("node:path");
const fs = require("node:fs");

const START = process.getCreationTime() ?? Date.now();
let readyAt = null;
const pendingNxm = process.argv.filter((a) => a.toLowerCase().startsWith("nxm://"));

if (!app.requestSingleInstanceLock()) {
  app.quit(); // the running instance receives our argv via second-instance
} else {
  let win = null;
  let web = null;
  const HOME = "https://www.nexusmods.com/games/skyrimspecialedition";

  app.on("second-instance", (_e, argv) => {
    for (const url of argv.filter((a) => a.toLowerCase().startsWith("nxm://"))) win?.webContents.send("nxm", url);
    if (win) { if (win.isMinimized()) win.restore(); win.focus(); }
  });

  const ensureWeb = () => {
    if (web) return web;
    web = new WebContentsView({ webPreferences: { partition: "persist:nexus" } });
    web.setBackgroundColor("#171b22");
    const report = () => win.webContents.send("browser-nav", web.webContents.getURL());
    web.webContents.on("did-navigate", report);
    web.webContents.on("did-navigate-in-page", report);
    // "Mod Manager Download" inside the embedded page: catch the nxm:// link
    // here instead of letting Windows route it through the protocol handler.
    const isNxm = (url) => url.toLowerCase().startsWith("nxm://");
    const catchNxm = (e) => { if (isNxm(e.url)) { e.preventDefault(); win.webContents.send("nxm", e.url); } };
    web.webContents.on("will-frame-navigate", catchNxm);
    web.webContents.setWindowOpenHandler(({ url }) => {
      if (isNxm(url)) win.webContents.send("nxm", url); else web.webContents.loadURL(url);
      return { action: "deny" };
    });
    web.webContents.loadURL(HOME);
    return web;
  };

  ipcMain.handle("info", () => ({ shell: "Electron", version: process.versions.electron, engine: `Chromium ${process.versions.chrome}`, renderer: "Chromium (Skia, GPU compositing)" }));
  ipcMain.handle("metrics", () => {
    const procs = app.getAppMetrics();
    return {
      workingSetMb: procs.reduce((s, p) => s + p.memory.workingSetSize, 0) / 1024,
      cpuPercent: procs.reduce((s, p) => s + p.cpu.percentCPUUsage, 0),
      processes: procs.length,
    };
  });
  ipcMain.handle("startup_ms", () => (readyAt ?? Date.now()) - START);
  ipcMain.handle("ready", () => {
    if (readyAt) return;
    readyAt = Date.now();
    if (process.env.LOOM_BENCH_READY_FILE) fs.writeFileSync(process.env.LOOM_BENCH_READY_FILE, String(readyAt - START));
  });
  ipcMain.handle("bench_scenario", () => process.env.LOOM_BENCH_SCENARIO || null);
  ipcMain.handle("take_nxm", () => pendingNxm.splice(0));
  ipcMain.handle("register_nxm", () => {
    const ok = app.isPackaged
      ? app.setAsDefaultProtocolClient("nxm")
      : app.setAsDefaultProtocolClient("nxm", process.execPath, [path.resolve(process.argv[1])]);
    return { ok, message: ok ? "Registered Loom (Electron) as the nxm:// handler for this user." : "Registration failed." };
  });
  ipcMain.handle("open_external", (_e, url) => shell.openExternal(url));
  ipcMain.handle("browser_show", (_e, r) => {
    const v = ensureWeb();
    if (!win.contentView.children.includes(v)) win.contentView.addChildView(v);
    v.setBounds(r);
  });
  ipcMain.handle("browser_hide", () => { if (web) win.contentView.removeChildView(web); });
  ipcMain.handle("browser_navigate", (_e, url) => ensureWeb().webContents.loadURL(url));
  ipcMain.handle("browser_back", () => web?.webContents.navigationHistory.goBack());
  ipcMain.handle("browser_forward", () => web?.webContents.navigationHistory.goForward());
  ipcMain.handle("browser_reload", () => web?.webContents.reload());

  Menu.setApplicationMenu(null);
  app.whenReady().then(() => {
    // LOOM_BENCH_WINDOW=x,y: test runs open the window there, without
    // activating it, so they never cover or take focus from the user's work.
    const pos = (process.env.LOOM_BENCH_WINDOW || "").split(",").map(Number);
    const benchPos = pos.length === 2 && pos.every(Number.isFinite);
    win = new BrowserWindow({
      width: 1280, height: 800, minWidth: 1024, minHeight: 640, useContentSize: true,
      title: "Loom", backgroundColor: "#0e1014",
      ...(benchPos ? { x: pos[0], y: pos[1], show: false } : {}),
      webPreferences: { preload: path.join(__dirname, "preload.js") },
    });
    if (benchPos) win.showInactive();
    win.loadFile(path.join(__dirname, "ui", "index.html"));
  });
  app.on("window-all-closed", () => app.quit());
}
