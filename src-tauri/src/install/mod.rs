//! One-click installation module for VibeShell skill configuration.
//!
//! This module provides functionality to detect and configure VibeShell skill
//! in various AI coding tools like Claude Code, Cursor, Codex, and Open Code.

pub mod detector;
pub mod installer;

pub use detector::*;
pub use installer::*;
