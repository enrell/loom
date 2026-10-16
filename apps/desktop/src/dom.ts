// Tiny DOM helpers: escaping, toasts, modals, size formatting.

export function esc(v: unknown): string {
  return String(v ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

export function $(sel: string, root: ParentNode = document): HTMLElement {
  const el = root.querySelector<HTMLElement>(sel);
  if (!el) throw new Error(`missing element ${sel}`);
  return el;
}

export function size(bytes: number): string {
  const mb = bytes / 1048576;
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

let toasts: HTMLElement | null = null;
export function toast(message: string, kind: "info" | "ok" | "error" = "info", ms = 4500): void {
  if (!toasts) {
    toasts = document.createElement("div");
    toasts.className = "toasts";
    document.body.appendChild(toasts);
  }
  const t = document.createElement("div");
  t.className = `toast ${kind}`;
  t.textContent = message;
  toasts.appendChild(t);
  setTimeout(() => t.remove(), kind === "error" ? ms * 2 : ms);
}

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/** Runs `f`, shows its error as a toast, returns undefined on failure. */
export async function attempt<T>(f: () => Promise<T>): Promise<T | undefined> {
  try {
    return await f();
  } catch (e) {
    toast(errorText(e), "error");
    return undefined;
  }
}

/** A modal with a form; resolves with the field values or null when cancelled. */
export function modal(title: string, body: string, ok = "OK"): Promise<Record<string, string> | null> {
  return new Promise((resolve) => {
    const o = document.createElement("div");
    o.className = "overlay";
    o.innerHTML = `<form class="modal"><h2>${esc(title)}</h2>${body}<div class="actions"><button type="button" class="btn" data-cancel>Cancel</button><button class="btn primary">${esc(ok)}</button></div></form>`;
    document.body.appendChild(o);
    const form = o.querySelector("form")!;
    const close = (v: Record<string, string> | null) => { o.remove(); resolve(v); };
    form.querySelector("[data-cancel]")!.addEventListener("click", () => close(null));
    o.addEventListener("mousedown", (e) => { if (e.target === o) close(null); });
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      close(Object.fromEntries(new FormData(form).entries()) as Record<string, string>);
    });
    form.querySelector<HTMLInputElement>("input, select")?.focus();
  });
}

export function confirmBox(title: string, message: string, ok = "Confirm"): Promise<boolean> {
  return modal(title, `<p class="dim">${esc(message)}</p>`, ok).then((v) => v !== null);
}
