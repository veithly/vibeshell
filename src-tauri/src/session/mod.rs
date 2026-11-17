pub mod manager;
#[allow(clippy::module_inception)]
pub mod session;

pub use manager::{SessionManager, SshCredential};
pub use session::{Session, SessionInfo, SessionState};
pub mod container;
#[cfg(test)]
mod container_integration_test;
