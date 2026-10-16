import "./style.css";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { mockListen } from "./mock";
import { api } from "./api";
import { toast } from "./dom";
import { rerender, startNxm } from "./pages/downloads";
import { onBrowserNavigated } from "./pages/nexus";
import { go, lastGame, mount, refreshSidebar, renderPage, selectGame, setRunning } from "./shell";
import { state, type PageId } from "./state";

const listen: typeof tauriListen = import.meta.env.MODE === "mock" ? (mockListen as unknown as typeof tauriListen) : tauriListen;

interface Progress { url: string; done: number; total: number | null }
interface Finished { url: string; error?: string; result?: { game: string; archive: string; installed?: { name: string } } }

async function boot(): Promise<void> {
  mount(document.getElementById("app")!);

  // nxm:// links: from Windows (second instance) or clicked inside the
  // embedded Nexus browser.
  await listen<string>("nxm", (e) => { toast("Download started"); startNxm(e.payload); });
  await listen<Progress>("download-progress", (e) => {
    const d = state.downloads.find((x) => x.url === e.payload.url && x.state === "running");
    if (d) { d.done = e.payload.done; d.total = e.payload.total; rerender(); }
  });
  await listen<Finished>("download-finished", async (e) => {
    const d = state.downloads.find((x) => x.url === e.payload.url && x.state === "running");
    if (d) {
      d.state = e.payload.error ? "error" : "done";
      d.message = e.payload.error ?? (e.payload.result?.installed ? `Installed ${e.payload.result.installed.name}` : "Downloaded");
    }
    if (e.payload.error) toast(e.payload.error, "error"); else toast(d?.message ?? "Downloaded", "ok");
    await refreshSidebar();
    if (state.page === "mods" || state.page === "downloads") renderPage();
  });
  await listen<string>("browser-nav", (e) => onBrowserNavigated(e.payload));
  await listen<{ game: string; error?: string; report?: { exit_code: number | null; captured: number } }>("launch-finished", async (e) => {
    setRunning(null);
    if (e.payload.error) toast(e.payload.error, "error");
    else toast(`Game closed${e.payload.report?.captured ? ` — ${e.payload.report.captured} new file(s) in overwrite` : ""}`, "ok");
    await refreshSidebar();
  });
  await listen<{ error?: string; report?: { output_mod?: string } }>("tool-finished", async (e) => {
    setRunning(null);
    if (e.payload.error) toast(e.payload.error, "error");
    else toast(`Tool finished${e.payload.report?.output_mod ? ` — output in ${e.payload.report.output_mod}` : ""}`, "ok");
    await refreshSidebar();
    if (state.page === "mods") renderPage();
  });

  const lib = await api.library();
  state.library = lib;
  const remembered = lastGame();
  const first = lib.games.find((g) => g.id === remembered && g.ok) ?? lib.games.find((g) => g.ok);
  if (first) await selectGame(first.id);
  else await go(lib.plugins ? "add-game" : "plugins");

  const start = await api.startPage();
  if (start) await go(start as PageId);
  for (const url of await api.takeNxm()) startNxm(url);
}

boot();
