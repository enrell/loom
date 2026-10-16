# Writing a Loom plugin

A plugin teaches Loom one game (or adds tools). It is a folder with a
`loom-plugin.toml` — **data only**: Loom refuses plugins that contain programs or
scripts, so installing one can never run code. Allowed files: `.toml`, `.md`,
`.txt`, `.json`, images, and `LICENSE`/`README`-style files; 5 MB at most.

Start from an official plugin in [`plugins/`](../plugins) that looks like your game:

| Game style | Start from |
|---|---|
| Bethesda (`Data/`, `Plugins.txt`) | `skyrimse`, `fallout4`; `oblivion` for timestamp ordering |
| One folder per mod (`Mods/<mod>/…`) | `stardewvalley`, `witcher3`, `bannerlord`, `rimworld` |
| Loose files under a fixed folder | `cyberpunk2077`, `hogwartslegacy`, `valheim` |

## `[plugin]`

| Key | |
|---|---|
| `id` | Import path, `github.com/<owner>/<repo>[/<subdir>]`. Must be where the plugin lives. |
| `version` | `MAJOR.MINOR.PATCH`. The git tag is `v<version>` (or `<subdir>/v<version>`). |
| `kind` | `game` (adds a game) or `tools` (adds `[[tools]]` for games other plugins provide). |
| `name`, `description`, `authors`, `license`, `homepage` | Shown in the marketplace. |

## `[game]`

| Key | |
|---|---|
| `id` | Lowercase slug, unique in the marketplace (`skyrimse`). |
| `nexus_domain` | The game's part of `nexusmods.com/<domain>`, for `nxm://` links. |
| `executables` | `{ id, name, path, args?, default?, optional? }`; paths relative to the game folder; exactly one `default`; script extenders and launchers `optional = true`. |

### `[game.detect]`

Any of `steam` (app ids), `gog` (product ids), `registry` (`{ key = 'HKLM\…', value = "…" }`),
`folder_names` (folder names under Steam libraries / GOG folders); plus `requires`,
files that must exist for a folder to count as the game.

### `[game.mods]` — where archive contents go

Mods are stored relative to the game folder. When installing, Loom walks the
archive breadth-first and stops at the first folder that matches:

- `root_markers` → that folder **is the game folder** (script extender DLLs, `bin/`, …);
- a folder named `data_dir` inside it → also the game folder;
- `data_markers` → that folder **is `data_dir`** (`*.esp`, `meshes`, …).

`fallback` decides when nothing matches: `data` (put it in `data_dir`), `root`, or
`package` — for games where every mod is its own folder: each folder that holds a
data marker (`manifest.json`, `SubModule.xml`) becomes `data_dir/<folder>`.
Markers are names or `*`/`?` globs, matched case-insensitively; a marker may be a
path (`archive/pc/mod`).

### `[game.load_order]` — optional

Only `format = "plugins-txt"` exists today (Bethesda-style).

| Key | |
|---|---|
| `plugins_file`, `loadorder_file` | Path templates: `%LOCALAPPDATA%`, `%APPDATA%`, `%USERPROFILE%`, `%DOCUMENTS%`, `%MYGAMES%`, `{game}`. |
| `extensions` | Plugin file extensions. |
| `star_prefix` | `*Name.esp` marks active entries (Skyrim SE, Fallout 4, Starfield). |
| `implicit` | Always loaded first, never written. |
| `light_extension`, `max_plugins` | Light plugins don't count toward the limit. |
| `header` | `tes4` or `tes3`: read masters and flags from plugin files. |
| `timestamps` | The game orders by file time: Loom sets the times of plugin files in its mod folders. |

## `[[tools]]`

Companion programs (translators, editors, generators). Loom finds the executable
(user tool folders, `search` templates, the game folder, enabled mods), runs it
**inside the game's virtual view** and saves what it writes as a
`Generated - <name>` mod (`output = "generated-mod"`, the default) or into the
overwrite folder. `{game}` in `args` is the game folder as the tool sees it.
`agent_notes` tells an agent when and how to run the tool. `homepage` is where users
get it — plugins never ship or download tools.

## Test and publish

```
loom plugin check ./my-plugin          # validates and prints the content hash
loom plugin add ./my-plugin            # install locally
loom game add mygame --path "D:\Games\My Game"
loom mod install mygame some-mod.zip   # check the layout it reports
loom doctor mygame
```

Then follow [marketplace/README.md](../marketplace/README.md).
