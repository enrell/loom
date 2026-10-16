import { api, type Problem } from "../api";
import { attempt, esc, toast } from "../dom";
import { enableReorder } from "../reorder";
import { refreshSidebar } from "../shell";
import { state, type Page } from "../state";

export const orderPage: Page = {
  needsGame: true,
  title: () => "Load order",
  async render(el) {
    const game = state.game!;
    const [order, problems] = await Promise.all([api.loadOrder(game), api.doctor(game)]);
    const issues = new Map<string, Problem[]>();
    for (const p of problems.filter((p) => ["missing-master", "inactive-master", "master-order"].includes(p.code))) {
      issues.set(p.subject.toLowerCase(), [...(issues.get(p.subject.toLowerCase()) ?? []), p]);
    }
    const floor = order.filter((p) => p.implicit).length;
    const active = order.filter((p) => p.active);
    el.innerHTML = `
      <div class="list" style="border-right:none;flex:1">
        <div class="toolbar">
          <span class="dim">${active.length} active · ${active.filter((p) => !p.is_light).length} full, ${active.filter((p) => p.is_light).length} light</span>
          <span class="spacer"></span>
          ${issues.size ? `<span class="badge danger">${issues.size} plugin(s) with problems</span>` : ""}
          <button class="btn" id="lo-sort"><i class="fi">&#xE8CB;</i>Sort by masters</button>
        </div>
        <div class="thead order-grid"><span></span><span>#</span><span></span><span>Plugin</span><span>Flags</span><span>From</span></div>
        <div class="rows" id="lo-rows">
          ${order.map((p, i) => {
            const probs = issues.get(p.name.toLowerCase()) ?? [];
            return `
            <div class="lrow order-grid" data-name="${esc(p.name)}" draggable="${p.implicit ? "false" : "true"}" data-base="${floor}" title="${esc(probs.map((x) => x.message).join("\n") || (p.masters.length ? "Masters: " + p.masters.join(", ") : ""))}">
              <span class="grip">${p.implicit ? "" : "&#xE76F;"}</span>
              <span class="dim">${i}</span>
              <span>${p.implicit ? '<span class="faint fi" title="always loaded">&#xE72E;</span>' : `<div class="toggle${p.active ? " on" : ""}" data-toggle></div>`}</span>
              <span class="name">${esc(p.name)} ${probs.length ? `<span class="badge danger">${esc(probs[0].code)}</span>` : ""}</span>
              <span>${p.is_master ? '<span class="badge info">M</span> ' : ""}${p.is_light ? '<span class="badge ok">L</span>' : ""}</span>
              <span class="dim name">${esc(p.provider === "base" ? "Game" : p.provider)}</span>
            </div>`;
          }).join("")}
        </div>
      </div>`;
    const rows = el.querySelector<HTMLElement>("#lo-rows")!;
    rows.addEventListener("click", async (e) => {
      if (!(e.target as HTMLElement).closest("[data-toggle]")) return;
      const name = (e.target as HTMLElement).closest<HTMLElement>(".lrow")!.dataset.name!;
      const p = order.find((x) => x.name === name)!;
      await attempt(() => api.pluginSetActive(game, name, !p.active));
      await orderPage.render(el);
      refreshSidebar();
    });
    enableReorder(rows, async (name, index) => {
      await attempt(() => api.pluginMove(game, name, index));
      await orderPage.render(el);
      refreshSidebar();
    });
    el.querySelector("#lo-sort")!.addEventListener("click", async () => {
      if (await attempt(() => api.loadOrderSort(game))) toast("Sorted: masters first, every plugin after its masters.", "ok");
      await orderPage.render(el);
      refreshSidebar();
    });
  },
};
