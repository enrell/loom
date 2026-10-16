// App state shared by the pages.
import type { Library, Overview } from "./api";

export type PageId = "mods" | "order" | "problems" | "downloads" | "nexus" | "tools" | "plugins" | "settings" | "add-game";

export interface Download { url: string; done: number; total: number | null; state: "running" | "done" | "error"; message?: string }

export const state = {
  library: null as Library | null,
  game: null as string | null,
  overview: null as Overview | null,
  page: "mods" as PageId,
  errors: 0,
  downloads: [] as Download[],
  running: null as string | null, // what is running (game or tool), for the status bar
};

/** Pages implement this; `render` fills the page element. */
export interface Page {
  title: () => string;
  render: (el: HTMLElement) => Promise<void> | void;
  /** Called when the page is left (e.g. to hide the embedded browser). */
  leave?: () => void;
  /** Needs a game to be selected. */
  needsGame: boolean;
}
