// Mock backend for headless UI work (`npm run build:mock`): answers the
// Tauri commands from fixtures recorded with the real `loom --json` CLI
// (scripts/ui-fixtures.py), so the UI renders in any browser — including
// headless Edge for screenshots and UI tests — without opening the app.
import fixtures from "../mock/fixtures.json";

const data = fixtures as Record<string, unknown>;

export async function mockInvoke<T>(cmd: string, _args?: Record<string, unknown>): Promise<T> {
  if (cmd === "start_page") return (new URLSearchParams(location.search).get("page") ?? null) as T;
  if (cmd === "take_nxm") return [] as T;
  if (cmd in data) return structuredClone(data[cmd]) as T;
  // Mutations succeed and change nothing.
  return undefined as T;
}

export async function mockListen(): Promise<() => void> {
  return () => {};
}
