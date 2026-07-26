#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod detector;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod manager;
#[cfg(any(target_os = "android", target_os = "ios"))]
mod mobile;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod session;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use detector::{detect_available_shells, get_default_shell, ShellInfo};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use manager::LocalShellManager;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub use mobile::{
    LocalShellInfo, LocalShellManager, LocalShellSession, LocalShellState, ShellInfo, ShellType,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use session::{LocalShellInfo, LocalShellSession, LocalShellState};
