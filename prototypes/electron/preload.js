// Exposes the host API the shared UI expects (see ui/app.js).
const { contextBridge, ipcRenderer } = require("electron");

const call = (name, arg) => ipcRenderer.invoke(name, arg);
const on = (channel, cb) => ipcRenderer.on(channel, (_e, payload) => cb(payload));

contextBridge.exposeInMainWorld("loom", {
  info: () => call("info"),
  metrics: () => call("metrics"),
  startupMs: () => call("startup_ms"),
  ready: () => call("ready"),
  benchScenario: () => call("bench_scenario"),
  takeNxm: () => call("take_nxm"),
  onNxm: (cb) => on("nxm", cb),
  registerNxm: () => call("register_nxm"),
  openExternal: (url) => call("open_external", url),
  browser: {
    supported: true,
    show: (r) => call("browser_show", r),
    hide: () => call("browser_hide"),
    navigate: (url) => call("browser_navigate", url),
    back: () => call("browser_back"),
    forward: () => call("browser_forward"),
    reload: () => call("browser_reload"),
    onNavigate: (cb) => on("browser-nav", cb),
  },
});
