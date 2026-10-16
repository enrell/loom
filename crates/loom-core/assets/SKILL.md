---
name: loom
description: Manage PC game mods with Loom — install game support, install and order mods, fix load orders and broken setups, download from Nexus Mods, run modding tools (translators, xEdit, BodySlide) and launch modded games. Use when the user asks about mods, load order, plugins (.esp/.esm/.esl), mod conflicts, Nexus Mods, or modding games like Skyrim, Fallout, Starfield, Cyberpunk 2077, The Witcher 3 or Stardew Valley.
---

# Loom

Loom is a mod manager that never touches the game folder: mods are projected into
the game through a virtual file system, and everything the game or a tool writes
goes to a per-profile overwrite folder or a "Generated - <tool>" mod.

Use the Loom MCP tools (`loom_*`). Without MCP, the `loom` command line does the
same things; add `--json` for machine-readable output.

## Concepts

- **Game support is a plugin.** If a game isn't available, search the reviewed
  marketplace (`loom_market_search`) and install its plugin (`loom_market_install`,
  hash-verified). Then set the game up (`loom_game_add`; it is detected
  automatically, or pass the install folder).
- **Mods are ordered; index 0 has the highest priority** and wins every file
  conflict. New mods go to the top, enabled.
- **Plugin load order** (Bethesda games: .esm/.esp/.esl) is separate from mod
  priority. Masters must load before the plugins that need them; game masters
  and Creation Club content are implicit and always first.
- **Profiles** are independent mod lists for the same game.

## Fixing a setup — always this loop

1. `loom_doctor` — every problem has a `code`, a `message` and usually a `fix`.
2. Apply the fix with the matching tool:
   - `missing-master`: the plugin needs a file nobody provides. Find and install
     the mod that provides it, or disable the plugin
     (`loom_load_order_set_active`, active=false).
   - `inactive-master`: activate the master (`loom_load_order_set_active`).
   - `master-order`: `loom_load_order_sort`.
   - `mod-missing`: the mod's folder is gone — `loom_mod_remove`.
   - `overwrite-not-empty`: offer `loom_overwrite_to_mod` so the files are kept as a mod.
   - `file-conflicts`: informational; inspect with `loom_conflicts` and reorder
     mods (`loom_mod_move`) only if the user wants a different winner.
3. `loom_doctor` again, until no `error` remains. Explain what you changed.

## Installing mods

- From a file: `loom_mod_install` with the archive path (.zip/.7z/.rar). The
  result's `layout` says where the content was placed — check it looks right.
- From Nexus Mods: `loom_nxm_download` with the nxm:// link (from the site's
  "Mod Manager Download" button). Needs the user's Nexus API key in Loom's settings.
- After installing a plugin-bearing mod, check `loom_load_order` and run the
  doctor loop.

## Tools (translators, editors, generators)

`loom_tools` lists the tools a game's plugin declares and whether each is
installed. `loom_tool_run` starts one inside the game's virtual view; whatever it
writes becomes the "Generated - <tool>" mod. Read each tool's `agent_notes`.

Translating a modlist (e.g. Skyrim with SSE Auto Translator): run the translator
tool, let it match every installed mod with its published translation, then keep
the generated mod above the mods it translates (index 0 is safest) and run the
doctor loop. Translation mods downloaded from Nexus should sit directly above the
mod they translate.

## Rules

- Never delete or move files in the game folder yourself; Loom never needs it.
- Ask before removing mods (`loom_mod_remove` is irreversible) or changing
  profiles the user didn't mention.
- Only call `loom_launch` when the user asked to play: it starts the game.
- Prefer reviewed marketplace plugins over `loom_plugin_add` with arbitrary
  import paths.
