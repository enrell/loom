import { open } from "@tauri-apps/plugin-dialog";
import { api, type MarketEntry } from "../api";
import { attempt, confirmBox, esc, toast } from "../dom";
import { refreshSidebar } from "../shell";
import type { Page } from "../state";

let query = "";
let market: MarketEntry[] | null = null;
let marketError = "";

export const pluginsPage: Page = {
  needsGame: false,
  title: () => "Plugins",
  async render(el) {
    const installed = await api.plugins();
    if (market === null) {
      try {
        market = await api.marketSearch("");
        marketError = "";
      } catch (e) {
        market = [];
        marketError = String(e);
      }
    }
    const have = new Set(installed.map((p) => p.lock.id));
    const q = query.toLowerCase();
    const shown = market.filter((e) => !q || e.name.toLowerCase().includes(q) || e.games.some((g) => g.includes(q)) || e.tags.some((t) => t.includes(q)));
    el.innerHTML = `
      <div class="content">
        <div class="card">
          <h3>Installed</h3>
          ${installed.length ? `<table class="table"><thead><tr><th style="width:34%">Plugin</th><th>Provides</th><th style="width:90px">Version</th><th style="width:100px"></th></tr></thead><tbody>
            ${installed.map((p) => `<tr><td><b>${esc(p.name)}</b><br><span class="faint mono">${esc(p.lock.id)}</span></td>
              <td class="dim">${p.game ? `game <b>${esc(p.game)}</b>` : ""}${p.tools ? ` · ${p.tools} tool(s)` : ""}</td>
              <td>${esc(p.lock.version)}</td>
              <td><button class="btn small danger" data-remove="${esc(p.lock.id)}">Remove</button></td></tr>`).join("")}
          </tbody></table>` : `<div class="empty"><b>No plugins yet</b>Games are supported through plugins. Install one below.</div>`}
          <div class="row" style="margin-top:12px"><button class="btn small" id="add-dir">Install from folder…</button><span class="faint">for plugin authors</span></div>
        </div>
        <div class="card">
          <h3>Marketplace — reviewed plugins</h3>
          <div class="row" style="margin-bottom:10px"><input class="input grow" id="mq" placeholder="Search games and tools" value="${esc(query)}" spellcheck="false"></div>
          ${marketError ? `<div class="dim">Marketplace unavailable: ${esc(marketError)}</div>` : ""}
          <table class="table"><tbody>
            ${shown.map((e) => `<tr><td style="width:36%"><b>${esc(e.name)}</b><br><span class="faint">${esc(e.games.join(", "))}</span></td>
              <td class="dim">${esc(e.description)}</td>
              <td style="width:80px">${esc(e.versions.at(-1)?.version ?? "")}</td>
              <td style="width:100px">${have.has(e.id) ? '<span class="badge ok">installed</span>' : `<button class="btn small primary" data-install="${esc(e.id)}">Install</button>`}</td></tr>`).join("")}
          </tbody></table>
        </div>
      </div>`;
    const mq = el.querySelector<HTMLInputElement>("#mq")!;
    mq.addEventListener("input", () => { query = mq.value; Promise.resolve(pluginsPage.render(el)).then(() => { const n = el.querySelector<HTMLInputElement>("#mq")!; n.focus(); n.setSelectionRange(n.value.length, n.value.length); }); });
    el.querySelectorAll<HTMLElement>("[data-install]").forEach((b) => b.addEventListener("click", async () => {
      b.classList.add("busy");
      const r = await attempt(() => api.marketInstall(b.dataset.install!));
      if (r) toast(`Installed ${r.id} ${r.version} (hash verified)`, "ok");
      await pluginsPage.render(el);
      refreshSidebar();
    }));
    el.querySelectorAll<HTMLElement>("[data-remove]").forEach((b) => b.addEventListener("click", async () => {
      const id = b.dataset.remove!;
      if (await confirmBox("Remove plugin", `Remove ${id}? Its game's mods and profiles are kept and come back when you re-install it.`, "Remove")) {
        await attempt(() => api.pluginRemove(id));
        await pluginsPage.render(el);
        refreshSidebar();
      }
    }));
    el.querySelector("#add-dir")!.addEventListener("click", async () => {
      const dir = await open({ directory: true, title: "Plugin folder (with loom-plugin.toml)" });
      if (typeof dir === "string") {
        const r = await attempt(() => api.pluginAddDir(dir));
        if (r) toast(`Installed ${r.id}`, "ok");
        await pluginsPage.render(el);
        refreshSidebar();
      }
    });
  },
};
