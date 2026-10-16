import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { esc } from "../dom";
import { go } from "../shell";
import { state, type Page } from "../state";

let url = "";
let host: HTMLElement | null = null;

function place(): void {
  if (!host?.isConnected) return;
  const r = host.getBoundingClientRect();
  api.browserShow({ x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) });
}
window.addEventListener("resize", () => { if (state.page === "nexus") place(); });

function home(): string {
  const d = state.overview?.nexus_domain;
  return d ? `https://www.nexusmods.com/games/${d}` : "https://www.nexusmods.com/games";
}

/** Shows the Nexus tab at `target`. */
export function openNexus(target: string): void {
  url = target;
  go("nexus");
}

export function onBrowserNavigated(u: string): void {
  url = u;
  const input = document.querySelector<HTMLInputElement>("#b-url");
  if (input) input.value = u;
}

export const nexusPage: Page = {
  needsGame: false,
  title: () => "Nexus Mods",
  render(el) {
    const target = url || home();
    el.innerHTML = `
      <div class="urlbar">
        <button class="iconbtn" id="b-back" title="Back">&#xE72B;</button>
        <button class="iconbtn" id="b-fwd" title="Forward">&#xE72A;</button>
        <button class="iconbtn" id="b-reload" title="Reload">&#xE72C;</button>
        <button class="iconbtn" id="b-home" title="Game page">&#xE80F;</button>
        <input class="input" id="b-url" spellcheck="false" value="${esc(target)}">
        <button class="btn" id="b-ext"><i class="fi">&#xE8A7;</i>Open in browser</button>
      </div>
      <div class="webhost" id="webhost"></div>`;
    host = el.querySelector("#webhost");
    const input = el.querySelector<HTMLInputElement>("#b-url")!;
    const nav = (u: string) => {
      if (!/^[a-z][a-z0-9+.-]*:/i.test(u)) u = "https://" + u;
      url = u;
      input.value = u;
      api.browserNavigate(u);
    };
    input.addEventListener("keydown", (e) => { if (e.key === "Enter") nav(input.value.trim()); });
    el.querySelector("#b-back")!.addEventListener("click", () => api.browserEval("history.back()"));
    el.querySelector("#b-fwd")!.addEventListener("click", () => api.browserEval("history.forward()"));
    el.querySelector("#b-reload")!.addEventListener("click", () => api.browserEval("location.reload()"));
    el.querySelector("#b-home")!.addEventListener("click", () => nav(home()));
    el.querySelector("#b-ext")!.addEventListener("click", () => openUrl(input.value));
    requestAnimationFrame(() => { place(); nav(target); });
  },
  leave() {
    api.browserHide();
  },
};
