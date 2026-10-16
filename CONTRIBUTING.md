# Contributing to Loom

Thanks for helping! The fastest ways to contribute:

## Add or improve a game

Game support lives in plugins, not in the core. Write or fix a
`loom-plugin.toml` ([docs/PLUGINS.md](docs/PLUGINS.md)), test it with
`loom plugin check`, `loom game add` and `loom doctor`, and either:

- improve an official plugin under [`plugins/`](plugins) (bump its `version`,
  run `python scripts/sync-marketplace.py`, open a PR), or
- publish your own repository and add it to the marketplace
  ([marketplace/README.md](marketplace/README.md)).

If you own a game whose plugin README says *not yet verified*, running
`loom game add <id>` and `loom doctor <id>` and reporting back is a great first
contribution.

## Work on the code

- Rust: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` must pass (CI runs them on Windows).
- The core never names a game. Game-specific behavior is a plugin setting; if a
  plugin needs something new, add a *generic* capability to `loom-core` and a
  field to the plugin format (`loom-plugin`), with validation and docs.
- Every user-facing operation is a method on `Loom`/`Game` in `loom-core`, exposed
  in the CLI, the MCP server and the desktop app. Keep the three in sync.
- Desktop UI: `apps/desktop` (TypeScript, no framework). `npm run check` must pass;
  `npm run build:mock` + `python scripts/ui-shots.py` gives headless screenshots.

## Principles

- Never write to the game folder.
- Plugins are data; they can't run code.
- Errors say what to do next.
