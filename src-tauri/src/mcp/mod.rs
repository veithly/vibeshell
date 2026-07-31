//! MCP (Model Context Protocol) Server Module
//!
//! This module implements an MCP server that exposes VibeShell's SSH/SFTP
//! functionality as tools for AI assistants.
//!
//! The desktop GUI hosts an authenticated HTTP Gateway on an ephemeral loopback
//! port and publishes per-launch discovery metadata. The stdio transport remains
//! in the core library for compatibility with direct embedders.

pub mod approval;
pub mod gateway;
pub mod guard;
pub mod server;
pub mod stdio;
pub mod tools;

pub use approval::{AgentApprovalManager, ApprovalEvent, ApprovalOutcome};
pub use gateway::{gateway_manifest_path, AgentGateway, AgentGatewayStatus};
pub use guard::{AgentInputTracker, GuardConfig, SharedAgentInputTracker};
pub use server::McpServer;
