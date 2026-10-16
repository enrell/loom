// Typed wrappers over the Tauri commands in src-tauri/src/main.rs.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { mockInvoke } from "./mock";

// `vite build --mode mock` answers from recorded fixtures instead (headless
// UI work); in normal builds the mock is dropped as dead code.
const invoke: typeof mockInvoke = import.meta.env.MODE === "mock" ? mockInvoke : tauriInvoke;

export interface GameRef { id: string; name: string; ok: boolean; profile?: string; error?: string }
export interface Library { home: string; games: GameRef[]; plugins: number }
export interface Candidate { path: string; source: string }
export interface AvailableGame { id: string; name: string; plugin: string; set_up: boolean; found: Candidate[] }
export interface Executable { id: string; name: string; path: string; default?: boolean; optional?: boolean }
export interface Locked { id: string; version: string; source: string; commit?: string; sum: string }
export interface Overview {
  id: string; name: string; path: string; profile: string; profiles: string[]; executables: Executable[];
  nexus_domain?: string; has_load_order: boolean; launch_mode: string; plugin: Locked;
}
export interface ModMeta { version?: string; source?: string; nexus?: { game: string; mod_id: number; file_id?: number }; generated_by?: string; installed: number }
export interface ModInfo { name: string; priority: number; enabled: boolean; files: number; bytes: number; meta: ModMeta; missing: boolean }
export interface Conflict { path: string; winner: string; losers: string[] }
export interface PluginEntry { name: string; active: boolean; implicit: boolean; provider: string; masters: string[]; is_master: boolean; is_light: boolean }
export interface Problem { severity: "error" | "warning" | "info"; code: string; message: string; subject: string; fix?: string }
export interface ToolInfo { id: string; name: string; description: string; homepage?: string; found?: string; output: string; agent_notes: string; plugin: string }
export interface InstalledPlugin { lock: Locked; name: string; game?: string; tools: number }
export interface MarketEntry { id: string; name: string; kind: string; games: string[]; description: string; tags: string[]; versions: { version: string; ref: string; sum: string; reviewed: string }[] }
export interface Settings { nexus_api_key_set: boolean; launch_mode?: string; marketplace: string; mount_available: boolean; home: string; agents_wizard_seen: boolean }
export interface Agent { id: string; name: string; installed: boolean; mcp: boolean; skill: boolean | null; config_path: string; homepage: string; install: string }
export interface AgentResult { id: string; ok: boolean; error?: string; report?: { config_path: string; skill_path?: string; via_cli: boolean; backup?: string } }

export const api = {
  library: () => invoke<Library>("library"),
  availableGames: () => invoke<AvailableGame[]>("available_games"),
  gameAdd: (id: string, path?: string) => invoke("game_add", { id, path: path ?? null }),
  overview: (id: string) => invoke<Overview>("game_overview", { id }),
  mods: (game: string) => invoke<ModInfo[]>("mods", { game }),
  modInstall: (game: string, source: string) => invoke<{ name: string; files: number; layout: string[] }>("mod_install", { game, source }),
  modSetEnabled: (game: string, name: string, enabled: boolean) => invoke("mod_set_enabled", { game, name, enabled }),
  modMove: (game: string, name: string, index: number) => invoke("mod_move", { game, name, index }),
  modRemove: (game: string, name: string) => invoke("mod_remove", { game, name }),
  conflicts: (game: string) => invoke<Conflict[]>("conflicts", { game }),
  loadOrder: (game: string) => invoke<PluginEntry[]>("load_order", { game }),
  pluginSetActive: (game: string, name: string, active: boolean) => invoke("plugin_set_active", { game, name, active }),
  pluginMove: (game: string, name: string, index: number) => invoke("plugin_move", { game, name, index }),
  loadOrderSort: (game: string) => invoke<PluginEntry[]>("load_order_sort", { game }),
  doctor: (game: string) => invoke<Problem[]>("doctor", { game }),
  profileSwitch: (game: string, name: string) => invoke("profile_switch", { game, name }),
  profileCreate: (game: string, name: string, from?: string) => invoke("profile_create", { game, name, from: from ?? null }),
  overwriteToMod: (game: string, name: string) => invoke<number>("overwrite_to_mod", { game, name }),
  launch: (game: string, exe?: string) => invoke("launch", { game, exe: exe ?? null, mode: null }),
  tools: (game: string) => invoke<ToolInfo[]>("tools", { game }),
  toolRun: (game: string, tool: string) => invoke("tool_run", { game, tool }),
  plugins: () => invoke<InstalledPlugin[]>("plugins"),
  marketSearch: (query: string) => invoke<MarketEntry[]>("market_search", { query }),
  marketInstall: (id: string) => invoke<Locked>("market_install", { id }),
  pluginAddDir: (dir: string) => invoke<Locked>("plugin_add_dir", { dir }),
  pluginRemove: (id: string) => invoke<boolean>("plugin_remove", { id }),
  settings: () => invoke<Settings>("settings"),
  settingsSet: (key: string, value: string | null) => invoke("settings_set", { key, value }),
  agentsList: () => invoke<Agent[]>("agents_list"),
  agentsSetup: (ids: string[]) => invoke<AgentResult[]>("agents_setup", { ids }),
  agentsRemove: (id: string) => invoke("agents_remove", { id }),
  agentsExportSkill: (dir: string) => invoke<string>("agents_export_skill", { dir }),
  agentsWizardSeen: () => invoke("agents_wizard_seen"),
  nxmDownload: (url: string) => invoke("nxm_download", { url }),
  takeNxm: () => invoke<string[]>("take_nxm"),
  startPage: () => invoke<string | null>("start_page"),
  registerNxm: () => invoke<string>("register_nxm"),
  browserShow: (r: { x: number; y: number; width: number; height: number }) => invoke("browser_show", r),
  browserHide: () => invoke("browser_hide"),
  browserNavigate: (url: string) => invoke("browser_navigate", { url }),
  browserEval: (js: string) => invoke("browser_eval", { js }),
};
