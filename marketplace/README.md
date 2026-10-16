# Loom marketplace

The marketplace is this folder: one TOML file per plugin in `plugins/`, each
pointing at the plugin's **import path** and listing the versions that passed
review, each pinned to a **content hash**. Loom downloads the plugin from its own
repository and refuses it unless its files hash to exactly what was reviewed.

```toml
# plugins/my-game.toml
id = "github.com/you/loom-my-game"      # import path (repository, optional sub-folder)
name = "My Game"
kind = "game"                           # or "tools"
games = ["mygame"]                      # game ids it provides
description = "Game support for My Game: detection, mod layout, load order."
tags = ["unity", "bepinex"]

[[versions]]
version = "1.0.0"                       # plugin.version in its loom-plugin.toml
ref = "v1.0.0"                          # git tag (or <subdir>/v1.0.0 in a monorepo)
sum = "sha256:…"                        # `loom plugin check <folder>`
reviewed = "2026-10-15"
```

## Adding a plugin

1. Write the plugin (see [docs/PLUGINS.md](../docs/PLUGINS.md)) in your own repository
   and tag a release: `v1.0.0` at the repository root, or `path/to/plugin/v1.0.0`
   for a plugin in a sub-folder — the same rule Go modules use.
2. Run `loom plugin check <folder>` on that tag's checkout and copy the `sum`.
3. Open a pull request adding `marketplace/plugins/<name>.toml`.

Automated checks then:

- validate every entry (`loom market validate marketplace`);
- download your plugin at `ref` from its import path and verify the `sum`, the
  manifest and the data-only rule (plugins can't contain programs or scripts);
- run the review agent against [REVIEW.md](REVIEW.md), which comments on safety,
  functionality and completeness.

A maintainer merges after a passing review. To publish a new version, add a new
`[[versions]]` block in a new pull request — reviewed versions are never edited.

## Removing a plugin

Delete its file (or a version) in a pull request. Users who already installed it
keep it until they remove it; it just stops being offered.
