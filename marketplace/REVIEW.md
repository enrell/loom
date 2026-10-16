# Marketplace review checklist

The review agent (and the maintainer) answers every question below for each
plugin version a pull request adds. The result is a comment with one section per
heading and a final verdict: **approve**, **request changes** or **reject**.

The automated checks have already confirmed that the entry is well formed, that
the plugin downloads from its import path at `ref`, that its content hash equals
`sum`, that its manifest validates and that it contains only data files.

## 1. Safety

- Does the plugin only *describe* the game? It must not try to make Loom run,
  download or execute anything beyond what the format allows.
- Do `[[tools]]` entries point at the tool's official page (`homepage`) and
  only look for executables by name? A plugin must never ship or fetch a tool.
- Are `game.detect` registry keys plausible for this game (no unrelated keys)?
- Do path templates stay inside the game folder or the game's own folders under
  `%LOCALAPPDATA%` / `%MYGAMES%`? Writing to another program's files is a reject.
- Do `[[tools]].args` only pass paths to the tool, never shell syntax?
- Is the license compatible with redistribution (any OSI license)?

## 2. Functionality

- Are the Steam ids / GOG ids / folder names correct for this game?
- Does `executables` name the real executables, with exactly one `default`?
  Script extenders and launchers marked `optional`?
- Does `game.mods` put files where the game loads them? Check `data_dir`, the
  markers and `fallback` against the game's real layout and two or three
  popular mods from its Nexus page.
- Load order (if present): right `plugins_file` path and format, `star_prefix`,
  implicit masters in the right order, `light_extension`, `max_plugins`,
  `header`, `timestamps` for games that order by file time.
- `nexus_domain` matches `nexusmods.com/<domain>`.

## 3. Completeness

- Is the game's most common modding setup covered (script extender / loader,
  frameworks, usual folders)? Missing pieces should be listed in the plugin's
  README as known limits.
- Are well-known companion tools declared, with `agent_notes` that tell an
  agent how and when to run them?
- Does the plugin's README say what was tested (game version, store)?
- Is the game id unique in the marketplace, and does `games` in the entry match
  `game.id` in the manifest?

## Verdict

- **approve**: no safety issue, functionality checked, gaps documented.
- **request changes**: fixable problems; list each with the line to change.
- **reject**: any safety problem that isn't a simple mistake.
