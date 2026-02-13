//! SFTP module for file transfer operations.
//!
//! This module provides SFTP client functionality built on top of russh-sftp,
//! enabling secure file transfer operations over SSH connections.

pub mod client;
pub mod operations;

pub use client::SftpClient;
pub use operations::*;
