import { api } from "../api";
import { esc } from "../dom";
import { state, type Page } from "../state";

const SEVERITY: Record<string, string> = { error: "danger", warning: "warn", info: "info" };

export const problemsPage: Page = {
  needsGame: true,
  title: () => "Problems",
  async render(el) {
    const problems = await api.doctor(state.game!);
    el.innerHTML = `
      <div class="content">
        <div class="card">
          <h3>Diagnosis</h3>
          ${problems.length ? problems.map((p) => `
            <div class="problem">
              <span class="badge ${SEVERITY[p.severity]}">${p.severity}</span>
              <div>
                <div>${esc(p.message)}</div>
                ${p.fix ? `<div class="fix faint">Fix: <span class="mono">${esc(p.fix)}</span></div>` : ""}
              </div>
              <span class="faint mono">${esc(p.code)}</span>
            </div>`).join("") : `<div class="empty"><b>No problems found</b>Everything checks out.</div>`}
        </div>
        <div class="card">
          <h3>Let an agent fix it</h3>
          <div class="dim">Every problem has a code and a fix command. An AI agent connected to Loom over MCP (<span class="mono">loom mcp</span>) can run the same diagnosis with <span class="mono">loom_doctor</span>, apply the fixes and check again until it's clean.</div>
        </div>
      </div>`;
  },
};
