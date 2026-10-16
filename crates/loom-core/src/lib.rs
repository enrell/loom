//! # loom-core
//!
//! Loom's engine. The core knows no game: everything game-specific comes from
//! declarative plugins (see `loom-plugin`), and the core provides the generic
//! capabilities they configure. See `docs/ARCHITECTURE.md`.
//!
//! [`Loom`] is the entry point; [`Game`] carries every per-game operation.
//! The CLI, the MCP server and the desktop app are thin layers over them.

pub mod agents;
pub mod archive;
pub mod config;
pub mod detect;
pub mod doctor;
pub mod error;
pub mod fsx;
pub mod launch;
pub mod library;
pub mod loadorder;
pub mod marketplace;
pub mod mods;
pub mod nexus;
pub mod paths;
pub mod plugins;
pub mod profile;
pub mod tools;

pub use error::{Error, Result};
pub use library::{Game, Instance, Loom};
pub use loom_plugin as plugin;
