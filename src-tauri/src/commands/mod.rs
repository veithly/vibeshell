pub mod agent;
pub mod app;
pub mod cloud_sync;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod coding_agent;
#[cfg(any(target_os = "android", target_os = "ios"))]
#[path = "coding_agent_mobile.rs"]
pub mod coding_agent;
pub mod dialog;
pub mod fingerprint;
pub mod install;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod local_shell;
#[cfg(any(target_os = "android", target_os = "ios"))]
#[path = "local_shell_mobile.rs"]
pub mod local_shell;
pub mod logging;
pub mod platform;
pub mod plugin;
pub mod server;
pub mod session;
pub mod sftp;
pub mod snippet;
pub mod tunnel;

pub use agent::*;
pub use app::*;
pub use cloud_sync::*;
pub use coding_agent::*;
pub use dialog::*;
pub use fingerprint::*;
pub use install::*;
pub use local_shell::*;
pub use logging::*;
pub use platform::*;
pub use plugin::*;
pub use server::*;
pub use session::*;
pub use sftp::*;
pub use snippet::*;
pub use tunnel::*;

// Re-export server status types for use in frontend
pub use session::{CpuInfo, DiskInfo, MemoryInfo, NetworkInfo, ServerStatus};
