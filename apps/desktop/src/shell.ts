// The app frame: sidebar (games + sections), top bar (profile, launch),
// status bar, and navigation between pages.
import { api } from "./api";
import { attempt, esc, modal, toast } from "./dom";
import { addGamePage } from "./pages/addgame";
import { downloadsPage } from "./pages/downloads";
import { modsPage } from "./pages/mods";
import { nexusPage } from "./pages/nexus";
import { orderPage } from "./pages/order";
import { pluginsPage } from "./pages/plugins";
import { problemsPage } from "./pages/problems";
import { settingsPage } from "./pages/settings";
import { toolsPage } from "./pages/tools";
import { state, type Page, type PageId } from "./state";

// Built on first use: the pages import this module too, so they aren't
// initialized yet while it loads.
let pages: Record<PageId, Page> | null = null;
function PAGE(id: PageId): Page {
  pages ??= {
    mods: modsPage, order: orderPage, problems: problemsPage, downloads: downloadsPage, nexus: nexusPage,
    tools: toolsPage, plugins: pluginsPage, settings: settingsPage, "add-game": addGamePage,
  };
  return pages[id];
}

const LAST_GAME = "loom.lastGame";

export function mount(root: HTMLElement): void {
  root.innerHTML = `
    <div class="app">
      <aside class="sidebar">
        <div class="brand"><div class="brand-mark">L</div><div><div class="brand-name">Loom</div><div class="brand-sub">mod manager</div></div></div>
        <nav class="nav" id="nav"></nav>
        <div class="sidebar-foot nav">
          <button class="nav-item" data-page="plugins"><i class="ico">&#xEA86;</i><span class="label">Plugins</span></button>
          <button class="nav-item" data-page="settings"><i class="ico">&#xE713;</i><span class="label">Settings</span></button>
        </div>
      </aside>
      <main class="main">
        <header class="topbar" id="topbar"></header>
        <section class="page" id="page"></section>
      </main>
      <footer class="status" id="status"></footer>
    </div>`;
  root.querySelector(".sidebar")!.addEventListener("click", (e) => {
    const item = (e.target as HTMLElement).closest<HTMLElement>("[data-page],[data-game],[data-add-game]");
    if (!item) return;
    if (item.dataset.game) selectGame(item.dataset.game);
    else if (item.dataset.addGame !== undefined) go("add-game");
    else go(item.dataset.page as PageId);
  });
}

export async function refreshSidebar(): Promise<void> {
  state.library = await api.library();
  if (state.game) {
    state.overview = await api.overview(state.game).catch(() => null);
    const problems = await api.doctor(state.game).catch(() => []);
    state.errors = problems.filter((p) => p.severity === "error").length;
  }
  const lib = state.library;
  const nav = document.getElementById("nav")!;
  const item = (page: PageId, icon: string, label: string, count = "") =>
    `<button class="nav-item${state.page === page ? " active" : ""}" data-page="${page}"><i class="ico">${icon}</i><span class="label">${label}</span>${count}</button>`;
  nav.innerHTML = `
    <div class="nav-label">Games <button data-add-game title="Add a game">+</button></div>
    ${lib.games.map((g) => `<button class="nav-item${g.id === state.game ? " active" : ""}" data-game="${esc(g.id)}" title="${esc(g.error ?? g.name)}"><i class="ico">&#xE7FC;</i><span class="label">${esc(g.name)}</span>${g.ok ? "" : '<span class="count err">!</span>'}</button>`).join("") || `<button class="nav-item" data-add-game><i class="ico">&#xE710;</i><span class="label">Add a game</span></button>`}
    ${state.game && state.overview ? `
      <div class="nav-label">${esc(state.overview.name)}</div>
      ${item("mods", "&#xE8F1;", "Mods")}
      ${state.overview.has_load_order ? item("order", "&#xE8FD;", "Load order") : ""}
      ${item("problems", "&#xE7BA;", "Problems", state.errors ? `<span class="count err">${state.errors}</span>` : "")}
      ${item("tools", "&#xE90F;", "Tools")}` : ""}
    <div class="nav-label">Nexus Mods</div>
    ${item("nexus", "&#xE774;", "Browse")}
    ${item("downloads", "&#xE896;", "Downloads", state.downloads.some((d) => d.state === "running") ? '<span class="count">…</span>' : "")}`;
  document.querySelectorAll<HTMLElement>(".sidebar-foot .nav-item").forEach((b) => b.classList.toggle("active", b.dataset.page === state.page));
  renderTopbar();
  renderStatus();
}

function renderTopbar(): void {
  const bar = document.getElementById("topbar")!;
  const page = PAGE(state.page);
  const o = state.overview;
  const gamePage = page.needsGame && o;
  bar.innerHTML = `
    <h1>${esc(page.title())}</h1>
    ${gamePage ? `<span class="sub">${esc(o.name)}</span>` : ""}
    <span class="spacer"></span>
    ${o ? `
      <select class="input" id="profile" title="Profile">
        ${o.profiles.map((p) => `<option value="${esc(p)}"${p === o.profile ? " selected" : ""}>${esc(p)}</option>`).join("")}
        <option value="__new">New profile…</option>
      </select>
      ${o.executables.length > 1 ? `<select class="input" id="exe" title="Executable">${o.executables.map((e) => `<option value="${esc(e.id)}"${e.default ? " selected" : ""}>${esc(e.name)}</option>`).join("")}</select>` : ""}
      <button class="btn primary${state.running ? " busy" : ""}" id="launch"><i class="fi">&#xE768;</i>${state.running ? "Running…" : "Play"}</button>` : ""}`;
  bar.querySelector<HTMLSelectElement>("#profile")?.addEventListener("change", async (e) => {
    const sel = e.target as HTMLSelectElement;
    if (sel.value === "__new") {
      const v = await modal("New profile", `<div class="field"><label>Name</label><input class="input" name="name" required></div>
        <div class="field"><label>Start from</label><select class="input" name="from"><option value="">Empty (all mods disabled)</option>${o!.profiles.map((p) => `<option value="${esc(p)}">Copy of ${esc(p)}</option>`).join("")}</select></div>`, "Create");
      if (v?.name) {
        if ((await attempt(() => api.profileCreate(state.game!, v.name, v.from || undefined))) !== undefined) await attempt(() => api.profileSwitch(state.game!, v.name));
      }
    } else {
      await attempt(() => api.profileSwitch(state.game!, sel.value));
    }
    await refreshSidebar();
    renderPage();
  });
  bar.querySelector("#launch")?.addEventListener("click", async () => {
    const exe = bar.querySelector<HTMLSelectElement>("#exe")?.value;
    if ((await attempt(() => api.launch(state.game!, exe))) !== undefined) {
      setRunning(state.overview!.name);
      toast(`Starting ${state.overview!.name} with profile ${state.overview!.profile}…`);
    }
  });
}

function renderStatus(): void {
  const o = state.overview;
  document.getElementById("status")!.innerHTML = `
    <span><span class="dot${state.running ? " busy" : ""}"></span><b>${state.running ? `Running ${esc(state.running)}` : "Ready"}</b></span>
    ${o ? `<span>${esc(o.name)} · ${esc(o.profile)}</span><span>${esc(o.launch_mode)} mode</span>` : ""}
    <span class="spacer"></span>
    ${state.library ? `<span>${state.library.plugins} plugin(s)</span>` : ""}`;
}

export function setRunning(what: string | null): void {
  state.running = what;
  renderTopbar();
  renderStatus();
}

let rendering = 0;
export async function renderPage(): Promise<void> {
  const el = document.getElementById("page")!;
  const page = PAGE(state.page);
  const token = ++rendering;
  if (page.needsGame && !state.game) {
    state.page = "add-game";
    return go("add-game");
  }
  try {
    await page.render(el);
  } catch (e) {
    if (token === rendering) el.innerHTML = `<div class="content"><div class="empty"><b>Something went wrong</b>${esc(String(e))}</div></div>`;
  }
}

export async function go(page: PageId): Promise<void> {
  if (state.page !== page) PAGE(state.page).leave?.();
  state.page = page;
  await refreshSidebar();
  await renderPage();
}

export async function selectGame(id: string): Promise<void> {
  state.game = id;
  try { localStorage.setItem(LAST_GAME, id); } catch { /* storage unavailable */ }
  const page = PAGE(state.page).needsGame || state.page === "add-game" ? "mods" : state.page;
  await go(page);
}

export function lastGame(): string | null {
  try { return localStorage.getItem(LAST_GAME); } catch { return null; }
}
