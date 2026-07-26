use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimePlatform {
    Macos,
    Windows,
    Linux,
    Ios,
    Android,
    Unknown,
}

impl RuntimePlatform {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "ios") {
            Self::Ios
        } else if cfg!(target_os = "android") {
            Self::Android
        } else {
            Self::Unknown
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilities {
    pub platform: RuntimePlatform,
    pub is_mobile: bool,
    pub window_controls: bool,
    pub local_shell: bool,
    pub agent_gateway: bool,
    pub desktop_updater: bool,
    pub cli_ipc: bool,
    pub directory_transfer: bool,
    pub background_tunnels: bool,
}

impl RuntimeCapabilities {
    pub fn current() -> Self {
        Self::for_platform(RuntimePlatform::current())
    }

    fn for_platform(platform: RuntimePlatform) -> Self {
        let is_mobile = matches!(platform, RuntimePlatform::Ios | RuntimePlatform::Android);
        let is_desktop = matches!(
            platform,
            RuntimePlatform::Macos | RuntimePlatform::Windows | RuntimePlatform::Linux
        );

        Self {
            platform,
            is_mobile,
            window_controls: is_desktop,
            local_shell: is_desktop,
            agent_gateway: is_desktop,
            desktop_updater: is_desktop,
            cli_ipc: is_desktop,
            directory_transfer: is_desktop,
            background_tunnels: is_desktop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_platforms_enable_desktop_only_capabilities() {
        let capabilities = RuntimeCapabilities::for_platform(RuntimePlatform::Linux);

        assert!(!capabilities.is_mobile);
        assert!(capabilities.window_controls);
        assert!(capabilities.local_shell);
        assert!(capabilities.agent_gateway);
        assert!(capabilities.desktop_updater);
        assert!(capabilities.cli_ipc);
        assert!(capabilities.directory_transfer);
        assert!(capabilities.background_tunnels);
    }

    #[test]
    fn mobile_platforms_disable_desktop_only_capabilities() {
        let capabilities = RuntimeCapabilities::for_platform(RuntimePlatform::Android);

        assert!(capabilities.is_mobile);
        assert!(!capabilities.window_controls);
        assert!(!capabilities.local_shell);
        assert!(!capabilities.agent_gateway);
        assert!(!capabilities.desktop_updater);
        assert!(!capabilities.cli_ipc);
        assert!(!capabilities.directory_transfer);
        assert!(!capabilities.background_tunnels);
    }

    #[test]
    fn serializes_for_the_frontend_contract() {
        let value = serde_json::to_value(RuntimeCapabilities::for_platform(RuntimePlatform::Macos))
            .unwrap();

        assert_eq!(value["platform"], "macos");
        assert_eq!(value["isMobile"], false);
        assert_eq!(value["windowControls"], true);
        assert_eq!(value["localShell"], true);
        assert_eq!(value["agentGateway"], true);
        assert_eq!(value["desktopUpdater"], true);
        assert_eq!(value["cliIpc"], true);
        assert_eq!(value["directoryTransfer"], true);
        assert_eq!(value["backgroundTunnels"], true);
    }
}
