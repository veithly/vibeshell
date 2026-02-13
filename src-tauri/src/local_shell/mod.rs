pub mod detector;
pub mod session;
pub mod manager;

pub use detector::{ShellInfo, detect_available_shells, get_default_shell};
pub use session::{LocalShellSession, LocalShellState, LocalShellInfo};
pub use manager::LocalShellManager;
