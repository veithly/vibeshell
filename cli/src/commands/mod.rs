//! CLI command implementations for VibeShell.
//!
//! This module contains subcommands for session management and SSH connections.
//! Commands communicate with the VibeShell GUI via IPC.

pub mod container;
pub mod file_tools;
pub mod import;
pub mod install;
pub mod plugins;
pub mod proxy;
pub mod server;
pub mod session;
pub mod sftp;
pub mod ssh;
