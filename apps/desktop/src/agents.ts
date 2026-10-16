// AI agents: the setup wizard (shown once, on first launch, and from
// Settings) and the Settings section to add or remove agents later.
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type Agent, type AgentResult } from "./api";
import { attempt, esc, toast } from "./dom";

const EXAMPLES = [
  "Check my Skyrim setup and fix any problems.",
  "Install the mods in my Downloads folder and sort the load order.",
  "Translate my modlist to Portuguese with SSE Auto Translator.",
];

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast("Copied to the clipboard", "ok");
  } catch {
    toast(text);
  }
}

function status(a: Agent): string {
  if (!a.installed) return '<span class="badge warn">not installed</span>';
  if (a.mcp) return `<span class="badge ok">set up</span>${a.skill === false ? ' <span class="badge warn">skill missing</span>' : ""}`;
  return '<span class="badge info">ready to set up</span>';
}

/** Buttons for an agent that isn't installed yet. */
function getIt(a: Agent): string {
  return `<button class="btn small" data-home="${esc(a.homepage)}">Get it</button>
    <button class="btn small" data-copy="${esc(a.install)}" title="${esc(a.install)}"><i class="fi">&#xE8C8;</i>Copy install command</button>`;
}

function wireCommon(root: HTMLElement): void {
  root.querySelectorAll<HTMLElement>("[data-home]").forEach((b) => b.addEventListener("click", () => openUrl(b.dataset.home!)));
  root.querySelectorAll<HTMLElement>("[data-copy]").forEach((b) => b.addEventListener("click", () => copy(b.dataset.copy!)));
}

// ---------------------------------------------------------------- wizard

/** Opens the setup wizard (at `step` 0 or 1); resolves when it closes. */
export function openAgentWizard(step = 0): Promise<void> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "overlay";
    overlay.innerHTML = `<div class="modal wizard"></div>`;
    document.body.appendChild(overlay);
    const box = overlay.querySelector<HTMLElement>(".wizard")!;
    let agents: Agent[] = [];
    let chosen = new Set<string>();
    let results: AgentResult[] = [];

    const close = () => {
      overlay.remove();
      api.agentsWizardSeen().catch(() => {});
      resolve();
    };

    const steps = (n: number) => `<div class="steps">${["Why", "Choose agents", "Done"].map((t, i) => `<span class="${i === n ? "on" : i < n ? "done" : ""}">${i + 1}. ${t}</span>`).join("")}</div>`;

    const intro = () => {
      box.innerHTML = `${steps(0)}
        <h2>Let an AI agent manage your mods</h2>
        <p class="dim">Connect a coding agent you already use — Claude Code, Codex, Cursor and others — to Loom. It gets:</p>
        <ul class="bullets">
          <li><b>Loom's MCP server</b>: every action in this app as a tool — install mods, sort load orders, run tools, download from Nexus.</li>
          <li><b>The Loom skill</b> (agents that read skills): how to diagnose and repair a setup with <span class="mono">loom_doctor</span>, and the rules to follow.</li>
        </ul>
        <p class="dim">Everything runs on your PC. The agent only acts when you ask it to, and Loom never touches your game folder.</p>
        <div class="actions"><button class="btn" data-skip>Not now</button><button class="btn primary" data-next>Choose agents</button></div>`;
      box.querySelector("[data-skip]")!.addEventListener("click", close);
      box.querySelector("[data-next]")!.addEventListener("click", choose);
    };

    const choose = async () => {
      box.innerHTML = `${steps(1)}<div class="empty">Looking for agents…</div>`;
      agents = (await attempt(() => api.agentsList())) ?? [];
      chosen = new Set(agents.filter((a) => a.installed && !a.mcp).map((a) => a.id));
      const anyInstalled = agents.some((a) => a.installed);
      box.innerHTML = `${steps(1)}
        <h2>Choose agents</h2>
        <p class="dim">${anyInstalled ? "Installed agents are selected. Ones you don't have yet can be added any time from <b>Settings → AI agents</b> after you install them." : "No agent found on this PC. Install one, then add it from <b>Settings → AI agents</b>."}</p>
        <div class="agent-list">
          ${agents.map((a) => `
            <label class="agent${a.installed ? "" : " missing"}">
              <input type="checkbox" value="${esc(a.id)}" ${chosen.has(a.id) ? "checked" : ""} ${a.installed ? "" : "disabled"}>
              <div class="grow"><b>${esc(a.name)}</b> ${status(a)}${a.installed ? "" : `<div class="row" style="margin-top:6px">${getIt(a)}</div>`}</div>
            </label>`).join("")}
        </div>
        <div class="actions"><button class="btn" data-back>Back</button><span class="spacer"></span><button class="btn" data-skip>Later</button><button class="btn primary" data-go>Set up</button></div>`;
      wireCommon(box);
      box.querySelectorAll<HTMLInputElement>("input[type=checkbox]").forEach((c) => c.addEventListener("change", () => {
        if (c.checked) chosen.add(c.value); else chosen.delete(c.value);
        refreshGo();
      }));
      const go = box.querySelector<HTMLButtonElement>("[data-go]")!;
      const refreshGo = () => { go.disabled = chosen.size === 0; };
      refreshGo();
      box.querySelector("[data-back]")!.addEventListener("click", intro);
      box.querySelector("[data-skip]")!.addEventListener("click", close);
      go.addEventListener("click", async () => {
        go.classList.add("busy");
        results = (await attempt(() => api.agentsSetup([...chosen]))) ?? [];
        done();
      });
    };

    const done = () => {
      const name = (id: string) => agents.find((a) => a.id === id)?.name ?? id;
      const ok = results.filter((r) => r.ok);
      box.innerHTML = `${steps(2)}
        <h2>${ok.length ? "Loom is connected" : "Nothing was set up"}</h2>
        <div class="agent-list">
          ${results.map((r) => `<div class="agent"><div class="grow"><b>${esc(name(r.id))}</b> ${r.ok ? '<span class="badge ok">set up</span>' : '<span class="badge danger">failed</span>'}
            <div class="faint mono" style="margin-top:4px">${esc(r.ok ? r.report?.config_path : r.error)}</div>
            ${r.ok && r.report?.skill_path ? `<div class="faint mono">skill: ${esc(r.report.skill_path)}</div>` : ""}</div></div>`).join("")}
        </div>
        ${ok.length ? `<p class="dim">Restart the agent${ok.length > 1 ? "s" : ""}, then try asking:</p>
          <div class="examples">${EXAMPLES.map((e) => `<button class="btn small" data-copy="${esc(e)}">“${esc(e)}”</button>`).join("")}</div>` : ""}
        <div class="actions"><button class="btn primary" data-close>Done</button></div>`;
      wireCommon(box);
      box.querySelector("[data-close]")!.addEventListener("click", close);
    };

    if (step === 1) choose(); else intro();
  });
}

// ---------------------------------------------------------------- settings section

/** Renders the Settings → AI agents card into `card`. */
export async function renderAgentsCard(card: HTMLElement): Promise<void> {
  const agents = (await attempt(() => api.agentsList())) ?? [];
  card.innerHTML = `
    <h3>AI agents</h3>
    <div class="dim" style="margin-bottom:10px">Agents connected to Loom get its MCP server and, where supported, the Loom skill. Install an agent any time and add it here.</div>
    <table class="table"><tbody>
      ${agents.map((a) => `<tr>
        <td style="width:26%"><b>${esc(a.name)}</b></td>
        <td style="width:24%">${status(a)}</td>
        <td class="row" style="justify-content:flex-end">${
          !a.installed ? getIt(a)
          : a.mcp ? `<button class="btn small" data-setup="${esc(a.id)}">Update</button><button class="btn small danger" data-remove="${esc(a.id)}">Remove</button>`
          : `<button class="btn small primary" data-setup="${esc(a.id)}">Set up</button>`
        }</td></tr>`).join("")}
    </tbody></table>
    <div class="row wrap" style="margin-top:12px">
      <button class="btn small" data-refresh>Check again</button>
      <button class="btn small" data-wizard>Open the setup wizard</button>
      <button class="btn small" data-export>Export the skill…</button>
      <span class="faint">or run <span class="mono">loom mcp</span> as an MCP server from any other client</span>
    </div>`;
  wireCommon(card);
  card.querySelectorAll<HTMLElement>("[data-setup]").forEach((b) => b.addEventListener("click", async () => {
    b.classList.add("busy");
    const [r] = (await attempt(() => api.agentsSetup([b.dataset.setup!]))) ?? [];
    if (r?.ok) toast("Set up — restart the agent to use Loom", "ok"); else if (r) toast(r.error ?? "Failed", "error");
    renderAgentsCard(card);
  }));
  card.querySelectorAll<HTMLElement>("[data-remove]").forEach((b) => b.addEventListener("click", async () => {
    await attempt(() => api.agentsRemove(b.dataset.remove!));
    renderAgentsCard(card);
  }));
  card.querySelector("[data-refresh]")!.addEventListener("click", () => renderAgentsCard(card));
  card.querySelector("[data-wizard]")!.addEventListener("click", async () => { await openAgentWizard(); renderAgentsCard(card); });
  card.querySelector("[data-export]")!.addEventListener("click", async () => {
    const dir = await open({ directory: true, title: "Where to save the Loom skill" });
    if (typeof dir === "string") {
      const p = await attempt(() => api.agentsExportSkill(dir));
      if (p) toast(`Saved ${p}`, "ok");
    }
  });
}
