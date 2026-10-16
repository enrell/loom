# Loom — architecture and MVP

Loom is an open-source mod manager. Mods never touch the game folder: Loom
projects them into the game process through [openvfs](https://github.com/enrell/openvfs),
and everything a person can do in the app an agent can do through the CLI or MCP.

## Principles

1. **The core knows no game.** Every game is a *plugin*. Removing a game is
   uninstalling its plugin — no core code changes, nothing else breaks.
2. **Plugins are data first.** A game plugin is a declarative `loom-plugin.toml`:
   detection, executables, mod layout, load-order format, tools. Data cannot run
   code, so community plugins are safe to review and safe to install.
3. **Capabilities live in the core, choices live in plugins.** Generic machinery
   that several games share — a Steam library scanner, a `plugins.txt`-style load
   order, an archive installer — is implemented once in the core and *selected and
   configured* by plugins. A capability never names a game.
4. **Go-style distribution.** A plugin is identified by its import path,
   `github.com/<owner>/<repo>[/<subdir>]`, and a version (a git tag:
   `v1.2.0`, or `<subdir>/v1.2.0` in a monorepo, exactly like Go modules). Loom
   records what it installed in `plugins.lock` with a content hash (like `go.sum`).
5. **The marketplace is a git repository of links.** One TOML file per plugin
   pointing at its import path and reviewed versions with their hashes. People add
   plugins by pull request; a review agent checks safety, functionality and
   completeness against [the review checklist](../marketplace/REVIEW.md) before a
   maintainer merges.
6. **Agents are first-class users.** Every operation is a core function exposed
   identically through the CLI (`--json`) and the MCP server. `loom doctor`
   produces machine-readable problems with suggested fixes, so an agent can find
   and repair a broken load order the same way a person would.
7. **Reversible by default.** The game folder is never written. Profiles are text,
   mods are folders, writes made while the game runs land in a per-profile
   overwrite folder; tools write into *generated mods* that can be toggled off.

## Layout

```
crates/
  loom-plugin/   plugin manifest schema + validation (shared with marketplace CI)
  loom-core/     library: instances, games, mods, profiles, conflicts, load order,
                 doctor, launch via openvfs, plugin manager, marketplace, Nexus
  loom-cli/      the `loom` binary (also hosts `loom mcp`)
  loom-mcp/      MCP server over loom-core
apps/desktop/    Tauri 2 shell + web UI (see prototypes/DECISION.md)
plugins/         official game plugins, one folder each
marketplace/     the plugin index (one TOML per plugin) + review checklist
```

## Data on disk

```
<library>/                      default %LOCALAPPDATA%\Loom, or LOOM_HOME
  config.toml                   settings (Nexus API key, openvfs location, …)
  plugins/<import-path>/        installed plugins
  plugins.lock                  installed plugin versions + hashes
  games/<game-id>/
    instance.toml               game folder, chosen plugin
    downloads/                  archives
    mods/<mod-name>/            one folder per mod, laid out relative to the GAME ROOT
      loom-mod.toml             source (nexus ids, archive), version, notes
    profiles/<profile>/
      profile.toml              ordered mod list (first = highest priority) + enabled flags
      loadorder.txt             plugin load order, when the game has one
      overwrite/                files the game or tools wrote while running
```

Mods are stored relative to the game root, so `Data\textures\x.dds` and
`skse64_loader.exe` live side by side in one mod. The virtual file system then
needs just one root: the game folder.

## Running the game

`loom launch` writes the plugin load order where the game reads it, then runs
the executable inside the profile's virtual view, through one of two openvfs
backends:

- **mount** (default when WinFsp is installed): the merged view — enabled mods in
  priority order over the game folder, with the overwrite folder as the writable
  layer — is mounted as a real folder and the game runs from there. Every process
  sees it, so script-extender loaders that start the game as a child work. For
  Steam games a `steam_appid.txt` exists in the view only, so the game doesn't
  restart itself from its real folder.
- **hook**: no driver. `openvfs-inject` starts the game with the openvfs hook DLL,
  which virtualizes the game folder inside that one process.

Either way, nothing on disk outside Loom's library changes — except the load
order file games keep in `%LOCALAPPDATA%`, which Loom writes and backs up once.

## Tools (e.g. a translation tool for Skyrim)

Plugins declare companion **tools**: how to find the executable, and what to do
with what it writes. `loom tool run <tool>` starts the tool *inside the same
virtual view the game gets* — it sees exactly the modded game — and captures its
output into a **generated mod** (`Generated: <tool>`). That gives every external
tool three properties for free:

- it works on the real merged data, without deploying anything;
- its output is a normal mod: visible, ordered, toggleable, deletable;
- an agent can drive it end to end over MCP (run tool → read the diff of the
  generated mod → keep it, reorder it, or roll it back).

A translation workflow is then: the tool (or an agent using the Nexus API) finds
the translation file for each installed mod, Loom installs each translation as a
mod placed right above the mod it translates, and the generated/translated output
stays separate from the originals.

## MVP

In:

- plugin manifest v1, validation, install from a local path or `github.com/...@version`,
  lock file, uninstall; marketplace index fetch, search, CI validation, review checklist
- official plugins for the most modded games (Bethesda titles, Cyberpunk 2077,
  The Witcher 3, Baldur's Gate 3, Stardew Valley, …)
- game detection (Steam libraries, registry hints, manual path)
- mods: install from archive or folder with layout detection, list, enable/disable,
  reorder, remove; file-conflict report
- profiles: create, switch, copy
- load order for the `plugins-txt` capability: read masters from plugin headers,
  enable/disable, reorder, sort by masters, write `Plugins.txt`/`loadorder.txt`
- `loom doctor`: missing masters, masters after dependents, missing mod folders,
  plugin limits, missing game/openvfs
- launch through openvfs (per-process view); tools run the same way into generated mods
- `nxm://` handling and Nexus downloads with a personal API key
- CLI with `--json` for everything; MCP server with the same operations
- desktop app: game picker, mods, load order, downloads, embedded Nexus browser, launch

Out (next): WASM plugins with code, LOOT masterlist sorting, Nexus SSO login,
collections, auto-update, Linux.
