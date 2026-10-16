import { api } from "../api";
import { attempt, esc, toast } from "../dom";
import type { Page } from "../state";

export const settingsPage: Page = {
  needsGame: false,
  title: () => "Settings",
  async render(el) {
    const s = await api.settings();
    el.innerHTML = `
      <div class="content">
        <form class="card" id="nexus-form">
          <h3>Nexus Mods</h3>
          <div class="dim">Your personal API key (nexusmods.com → Site preferences → API) lets Loom download mods. It's stored in your Loom library's config.toml.</div>
          <div class="row" style="margin-top:10px">
            <input class="input grow" name="key" type="password" placeholder="${s.nexus_api_key_set ? "A key is saved — paste a new one to replace it" : "Paste your API key"}" autocomplete="off">
            <button class="btn primary">Save</button>
            ${s.nexus_api_key_set ? '<button class="btn danger" type="button" id="key-clear">Forget</button>' : ""}
          </div>
        </form>
        <div class="card">
          <h3>Running games</h3>
          <div class="row">
            <select class="input" id="mode">
              <option value="auto">Automatic (${s.mount_available ? "mount — WinFsp found" : "hook — WinFsp not found"})</option>
              <option value="mount">Mount (WinFsp): every process sees the mods</option>
              <option value="hook">Hook (no driver): only the game process</option>
            </select>
          </div>
          ${s.mount_available ? "" : `<div class="dim" style="margin-top:8px">Install WinFsp for the most compatible mode: <span class="mono">winget install WinFsp.WinFsp</span></div>`}
        </div>
        <div class="card"><h3>Library</h3><div class="kv"><span class="k">Folder</span><span class="mono">${esc(s.home)}</span><span class="k">Marketplace</span><span class="mono">${esc(s.marketplace)}</span></div></div>
        <div class="card"><h3>Agents</h3><div class="dim">Connect an AI agent (Claude, Cursor, VS Code…) with the MCP server: command <span class="mono">loom mcp</span>. It gets the same powers you have here, plus <span class="mono">loom_doctor</span> to find and fix problems.</div></div>
      </div>`;
    const mode = el.querySelector<HTMLSelectElement>("#mode")!;
    mode.value = s.launch_mode ?? "auto";
    mode.addEventListener("change", async () => { await attempt(() => api.settingsSet("launch_mode", mode.value)); toast("Saved", "ok"); });
    el.querySelector<HTMLFormElement>("#nexus-form")!.addEventListener("submit", async (e) => {
      e.preventDefault();
      const key = new FormData(e.target as HTMLFormElement).get("key") as string;
      if (!key.trim()) return;
      if (await attempt(() => api.settingsSet("nexus_api_key", key.trim())) !== undefined) toast("Nexus API key saved", "ok");
      settingsPage.render(el);
    });
    el.querySelector("#key-clear")?.addEventListener("click", async () => { await attempt(() => api.settingsSet("nexus_api_key", null)); settingsPage.render(el); });
  },
};
