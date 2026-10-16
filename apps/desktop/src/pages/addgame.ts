import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { attempt, esc, toast } from "../dom";
import { go, refreshSidebar, selectGame } from "../shell";
import type { Page } from "../state";

export const addGamePage: Page = {
  needsGame: false,
  title: () => "Add a game",
  async render(el) {
    el.innerHTML = `<div class="content"><div class="empty">Looking for games…</div></div>`;
    const games = await api.availableGames();
    el.innerHTML = `
      <div class="content">
        <div class="card">
          <h3>Supported by your plugins</h3>
          ${games.length ? `<table class="table"><tbody>
            ${games.map((g) => `<tr>
              <td style="width:34%"><b>${esc(g.name)}</b><br><span class="faint">${esc(g.id)}</span></td>
              <td class="dim">${g.set_up ? "Set up" : g.found.length ? `Found (${esc(g.found[0].source)}): <span class="mono">${esc(g.found[0].path)}</span>` : "Not found on this PC"}</td>
              <td style="width:220px">${g.set_up ? `<button class="btn small" data-open="${esc(g.id)}">Open</button>` : `
                ${g.found.length ? `<button class="btn small primary" data-add="${esc(g.id)}">Add</button>` : ""}
                <button class="btn small" data-pick="${esc(g.id)}">Choose folder…</button>`}</td>
            </tr>`).join("")}
          </tbody></table>` : `<div class="empty"><b>No game plugins installed</b>Install support for your games from the marketplace.</div>`}
          <div class="row" style="margin-top:12px"><button class="btn" id="to-plugins">Browse the marketplace</button></div>
        </div>
      </div>`;
    const added = async (id: string, path?: string) => {
      if ((await attempt(() => api.gameAdd(id, path))) !== undefined) {
        toast("Game added", "ok");
        await refreshSidebar();
        selectGame(id);
      }
    };
    el.querySelectorAll<HTMLElement>("[data-add]").forEach((b) => b.addEventListener("click", () => added(b.dataset.add!)));
    el.querySelectorAll<HTMLElement>("[data-open]").forEach((b) => b.addEventListener("click", () => selectGame(b.dataset.open!)));
    el.querySelectorAll<HTMLElement>("[data-pick]").forEach((b) => b.addEventListener("click", async () => {
      const dir = await open({ directory: true, title: "Game folder" });
      if (typeof dir === "string") added(b.dataset.pick!, dir);
    }));
    el.querySelector("#to-plugins")!.addEventListener("click", () => go("plugins"));
  },
};
