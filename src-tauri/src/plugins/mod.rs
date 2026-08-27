//! Plugin facade.
//!
//! The manifest specification — types, validation, command rendering, and the
//! built-in catalog — lives in the `vibeshell-plugins` workspace crate so it
//! can evolve independently of the desktop app and be reused by other
//! workspace members (CLI, future tooling). This module re-exports it to keep
//! existing `crate::plugins::*` call sites stable.

pub use vibeshell_plugins::*;
