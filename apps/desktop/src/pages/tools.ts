import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { attempt, esc, toast } from "../dom";
import { state, type Page } from "../state";
import { setRunning } from "../shell";

export const toolsPage: Page = {
  needsGame: true,
  title: () => "Tools",
  async render(el) {
    const tools = await api.tools(state.game!);
    el.innerHTML = `
      <div class="content">
        <div class="card">
          <h3>Tools run inside the game's virtual view</h3>
          <div class="dim">A tool sees the game exactly as modded. What it writes becomes a <b>Generated</b> mod you can reorder, disable or delete — the game folder and your mods stay untouched.</div>
        </div>
        ${tools.length ? `<div class="card"><table class="table"><thead><tr><th style="width:26%">Tool</th><th>About</th><th style="width:30%">Status</th><th style="width:120px"></th></tr></thead><tbody>
          ${tools.map((t) => `<tr>
            <td><b>${esc(t.name)}</b></td>
            <td class="dim">${esc(t.description)}</td>
            <td>${t.found ? `<span class="badge ok">found</span> <span class="faint mono" title="${esc(t.found)}">${esc(t.found.split(/[\\/]/).pop())}</span>` : `<span class="badge warn">not installed</span>`}</td>
            <td>${t.found ? `<button class="btn small primary" data-run="${esc(t.id)}">Run</button>` : t.homepage ? `<button class="btn small" data-get="${esc(t.homepage)}">Get it</button>` : ""}</td>
          </tr>`).join("")}
        </tbody></table></div>` : `<div class="empty"><b>No tools for this game</b>Tools come from plugins; a tools plugin from the marketplace can add more.</div>`}
      </div>`;
    el.querySelectorAll<HTMLElement>("[data-run]").forEach((b) => b.addEventListener("click", async () => {
      const id = b.dataset.run!;
      if (await attempt(() => api.toolRun(state.game!, id)) !== undefined) {
        setRunning(tools.find((t) => t.id === id)?.name ?? id);
        toast("Running in the virtual view — close the tool when you're done.");
      }
    }));
    el.querySelectorAll<HTMLElement>("[data-get]").forEach((b) => b.addEventListener("click", () => openUrl(b.dataset.get!)));
  },
};
