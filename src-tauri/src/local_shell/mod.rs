pub mod detector;
pub mod manager;
pub mod session;

pub use detector::{detect_available_shells, get_default_shell, ShellInfo};
pub use manager::LocalShellManager;
pub use session::{LocalShellInfo, LocalShellSession, LocalShellState};
