import { api } from "../api";
import { attempt, esc, size, toast } from "../dom";
import { state, type Page } from "../state";

/** Starts downloading an nxm:// link (progress arrives through events). */
export async function startNxm(url: string): Promise<void> {
  if (state.downloads.some((d) => d.url === url && d.state === "running")) return;
  state.downloads.unshift({ url, done: 0, total: null, state: "running" });
  const ok = await attempt(() => api.nxmDownload(url));
  if (ok === undefined) state.downloads[0].state = "error";
  rerender();
}

let current: HTMLElement | null = null;
export function rerender(): void {
  if (current?.isConnected) downloadsPage.render(current);
}

function describe(url: string): string {
  const m = /^nxm:\/\/([^/]+)\/mods\/(\d+)\/files\/(\d+)/i.exec(url);
  return m ? `${m[1]} · mod ${m[2]} · file ${m[3]}` : url;
}

export const downloadsPage: Page = {
  needsGame: false,
  title: () => "Downloads",
  async render(el) {
    current = el;
    const settings = await api.settings();
    el.innerHTML = `
      <div class="content">
        ${settings.nexus_api_key_set ? "" : `<div class="card"><h3>Connect Nexus Mods</h3><div class="dim">Downloads need your personal API key (nexusmods.com → Site preferences → API). Add it in Settings.</div></div>`}
        <div class="row">
          <input class="input grow" id="nxm-url" placeholder="Paste an nxm:// link" spellcheck="false">
          <button class="btn primary" id="nxm-go">Download</button>
          <button class="btn" id="nxm-reg">Handle nxm:// links</button>
        </div>
        <div class="card">
          <h3>Downloads</h3>
          ${state.downloads.length ? `<table class="table"><thead><tr><th style="width:40%">Link</th><th>Progress</th><th style="width:30%">Result</th></tr></thead><tbody>
            ${state.downloads.map((d) => {
              const pct = d.total ? Math.round((d.done / d.total) * 100) : d.state === "done" ? 100 : 0;
              return `<tr><td class="mono">${esc(describe(d.url))}</td>
                <td>${d.state === "running" ? `<div class="progress"><div style="width:${pct}%"></div></div><span class="faint">${size(d.done)}${d.total ? " / " + size(d.total) : ""}</span>` : `<span class="badge ${d.state === "done" ? "ok" : "danger"}">${d.state}</span>`}</td>
                <td class="dim">${esc(d.message ?? "")}</td></tr>`;
            }).join("")}</tbody></table>` : `<div class="empty"><b>Nothing downloaded yet</b>Click “Mod Manager Download” on a Nexus Mods page (in the Nexus Mods tab or your browser).</div>`}
        </div>
      </div>`;
    const input = el.querySelector<HTMLInputElement>("#nxm-url")!;
    const go = () => { if (input.value.trim()) startNxm(input.value.trim()); };
    el.querySelector("#nxm-go")!.addEventListener("click", go);
    input.addEventListener("keydown", (e) => { if (e.key === "Enter") go(); });
    el.querySelector("#nxm-reg")!.addEventListener("click", async () => {
      const m = await attempt(() => api.registerNxm());
      if (m) toast(m, "ok");
    });
  },
};
