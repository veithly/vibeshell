//! MCP (Model Context Protocol) Server Module
//!
//! This module implements an MCP server that exposes VibeShell's SSH/SFTP
//! functionality as tools for AI assistants.

pub mod server;
pub mod tools;

pub use server::McpServer;
