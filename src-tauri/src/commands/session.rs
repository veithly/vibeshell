use serde::{Deserialize, Serialize};
use tauri::{State, AppHandle, Emitter};
use std::process::Command;
use std::sync::Arc;

use crate::local_shell::LocalShellManager;
use crate::session::{SessionManager, SessionInfo, SshCredential};
use crate::ssh::PtyConfig;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub server_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub server_name: String,
    pub auth_type: String,  // "password" or "key"
    pub credential: String, // password or private key content
    pub passphrase: Option<String>, // for encrypted keys
    pub cols: Option<u32>,
    pub rows: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOutputEvent {
    pub session_id: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendInputRequest {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendBytesRequest {
    pub session_id: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeRequest {
    pub session_id: String,
    pub cols: u32,
    pub rows: u32,
}

/// List all active sessions
#[tauri::command]
pub async fn session_list(
    manager: State<'_, Arc<SessionManager>>,
) -> Result<Vec<SessionInfo>, String> {
    Ok(manager.list().await)
}

/// Create a new session for a server by name (without connecting)
#[tauri::command]
pub async fn session_create(
    manager: State<'_, Arc<SessionManager>>,
    request: CreateSessionRequest,
) -> Result<SessionInfo, String> {
    let session = manager
        .create_by_name(&request.server_name)
        .await
        .map_err(|e| e.to_string())?;

    Ok(session.get_info().await)
}

/// Create and connect a new SSH session with credentials
#[tauri::command]
pub async fn session_connect(
    app: AppHandle,
    manager: State<'_, Arc<SessionManager>>,
    request: ConnectRequest,
) -> Result<SessionInfo, String> {
    // Parse credentials based on auth type
    let ssh_credential = match request.auth_type.as_str() {
        "password" => SshCredential::Password(request.credential),
        "key" => SshCredential::PrivateKey {
            key: request.credential,
            passphrase: request.passphrase,
        },
        _ => return Err(format!("Unknown auth type: {}", request.auth_type)),
    };

    // Configure PTY
    let pty_config = Some(PtyConfig {
        term: "xterm-256color".to_string(),
        cols: request.cols.unwrap_or(80),
        rows: request.rows.unwrap_or(24),
        pix_width: 0,
        pix_height: 0,
    });

    // Create and connect session
    let session = manager
        .create_with_credentials(&request.server_name, ssh_credential, pty_config)
        .await
        .map_err(|e| e.to_string())?;

    let session_id = session.id.clone();
    let info = session.get_info().await;

    // Subscribe to session output and emit events
    let mut receiver = session.subscribe();
    tokio::spawn(async move {
        while let Ok(data) = receiver.recv().await {
            let event = SessionOutputEvent {
                session_id: session_id.clone(),
                data,
            };
            // Emit to frontend
            let _ = app.emit("session-output", event);
        }
    });

    Ok(info)
}

/// Kill a specific session by ID
#[tauri::command]
pub async fn session_kill(
    manager: State<'_, Arc<SessionManager>>,
    request: SessionIdRequest,
) -> Result<(), String> {
    manager
        .kill(&request.session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Kill all active sessions
#[tauri::command]
pub async fn session_kill_all(
    manager: State<'_, Arc<SessionManager>>,
) -> Result<(), String> {
    manager
        .kill_all()
        .await
        .map_err(|e| e.to_string())
}

/// Send input data to a session (as string)
#[tauri::command]
pub async fn session_send_input(
    manager: State<'_, Arc<SessionManager>>,
    request: SendInputRequest,
) -> Result<(), String> {
    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session
        .write_to_ssh(request.data.as_bytes())
        .await
        .map_err(|e| e.to_string())
}

/// Send raw input data to a session (as bytes)
#[tauri::command]
pub async fn session_send_bytes(
    manager: State<'_, Arc<SessionManager>>,
    request: SendBytesRequest,
) -> Result<(), String> {
    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session
        .write_to_ssh(&request.data)
        .await
        .map_err(|e| e.to_string())
}

/// Resize a session's terminal
#[tauri::command]
pub async fn session_resize(
    manager: State<'_, Arc<SessionManager>>,
    request: ResizeRequest,
) -> Result<(), String> {
    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session
        .resize_pty(request.cols, request.rows)
        .await
        .map_err(|e| e.to_string())
}

/// Attach to a session and start receiving output events
#[tauri::command]
pub async fn session_attach(
    app: AppHandle,
    manager: State<'_, Arc<SessionManager>>,
    request: SessionIdRequest,
) -> Result<SessionInfo, String> {
    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session.attach().await;

    let sid = request.session_id.clone();
    let mut receiver = session.subscribe();

    // Spawn task to forward output to frontend
    tokio::spawn(async move {
        while let Ok(data) = receiver.recv().await {
            let event = SessionOutputEvent {
                session_id: sid.clone(),
                data,
            };
            let _ = app.emit("session-output", event);
        }
    });

    Ok(session.get_info().await)
}

/// Detach from a session
#[tauri::command]
pub async fn session_detach(
    manager: State<'_, Arc<SessionManager>>,
    request: SessionIdRequest,
) -> Result<(), String> {
    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session.detach().await;
    Ok(())
}

// =============================================================================
// Server Status Monitoring Types and Commands
// =============================================================================

/// CPU usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo {
    /// Overall CPU usage percentage (0-100)
    pub usage_percent: f64,
    /// Number of CPU cores
    pub core_count: u32,
    /// Load average (1 min, 5 min, 15 min)
    pub load_average: [f64; 3],
}

/// Memory usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    /// Total memory in bytes
    pub total: u64,
    /// Used memory in bytes
    pub used: u64,
    /// Free memory in bytes
    pub free: u64,
    /// Available memory in bytes
    pub available: u64,
    /// Usage percentage (0-100)
    pub usage_percent: f64,
    /// Swap total in bytes
    pub swap_total: u64,
    /// Swap used in bytes
    pub swap_used: u64,
}

/// Disk usage information for a mount point
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    /// Mount point path
    pub mount_point: String,
    /// Filesystem type
    pub filesystem: String,
    /// Total size in bytes
    pub total: u64,
    /// Used space in bytes
    pub used: u64,
    /// Available space in bytes
    pub available: u64,
    /// Usage percentage (0-100)
    pub usage_percent: f64,
}

/// Network interface statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    /// Interface name
    pub interface: String,
    /// Bytes received
    pub rx_bytes: u64,
    /// Bytes transmitted
    pub tx_bytes: u64,
    /// Packets received
    pub rx_packets: u64,
    /// Packets transmitted
    pub tx_packets: u64,
}

/// Complete server status information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    /// Server hostname
    pub hostname: String,
    /// System uptime in seconds
    pub uptime_seconds: u64,
    /// CPU information
    pub cpu: CpuInfo,
    /// Memory information
    pub memory: MemoryInfo,
    /// Disk information (one entry per mount point)
    pub disks: Vec<DiskInfo>,
    /// Network interface information
    pub network: Vec<NetworkInfo>,
    /// Timestamp when this status was collected (Unix timestamp)
    pub collected_at: i64,
}

/// Parse the output of /proc/stat to get CPU usage
fn parse_cpu_usage(stat_output: &str, num_cpus_output: &str) -> CpuInfo {
    let mut usage_percent = 0.0;
    let mut core_count = 1u32;
    let load_average = [0.0, 0.0, 0.0];

    // Parse core count
    if let Ok(count) = num_cpus_output.trim().parse::<u32>() {
        core_count = count;
    }

    // Parse CPU usage from /proc/stat
    // The first line is: cpu  user nice system idle iowait irq softirq steal guest guest_nice
    for line in stat_output.lines() {
        if line.starts_with("cpu ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                let user: u64 = parts[1].parse().unwrap_or(0);
                let nice: u64 = parts[2].parse().unwrap_or(0);
                let system: u64 = parts[3].parse().unwrap_or(0);
                let idle: u64 = parts[4].parse().unwrap_or(0);
                let iowait: u64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);

                let total = user + nice + system + idle + iowait;
                let used = user + nice + system;

                if total > 0 {
                    usage_percent = (used as f64 / total as f64) * 100.0;
                }
            }
            break;
        }
    }

    CpuInfo {
        usage_percent,
        core_count,
        load_average,
    }
}

/// Parse the output of /proc/loadavg
fn parse_load_average(loadavg_output: &str) -> [f64; 3] {
    let parts: Vec<&str> = loadavg_output.split_whitespace().collect();
    let mut load = [0.0, 0.0, 0.0];

    if parts.len() >= 3 {
        load[0] = parts[0].parse().unwrap_or(0.0);
        load[1] = parts[1].parse().unwrap_or(0.0);
        load[2] = parts[2].parse().unwrap_or(0.0);
    }

    load
}

/// Parse the output of /proc/meminfo
fn parse_memory_info(meminfo_output: &str) -> MemoryInfo {
    let mut total = 0u64;
    let mut free = 0u64;
    let mut available = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;

    for line in meminfo_output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let value: u64 = parts[1].parse().unwrap_or(0) * 1024; // Convert kB to bytes
            match parts[0] {
                "MemTotal:" => total = value,
                "MemFree:" => free = value,
                "MemAvailable:" => available = value,
                "Buffers:" => buffers = value,
                "Cached:" => cached = value,
                "SwapTotal:" => swap_total = value,
                "SwapFree:" => swap_free = value,
                _ => {}
            }
        }
    }

    // If MemAvailable is not present (older kernels), estimate it
    if available == 0 {
        available = free + buffers + cached;
    }

    let used = total.saturating_sub(available);
    let usage_percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    MemoryInfo {
        total,
        used,
        free,
        available,
        usage_percent,
        swap_total,
        swap_used: swap_total.saturating_sub(swap_free),
    }
}

/// Parse the output of df command
fn parse_disk_info(df_output: &str) -> Vec<DiskInfo> {
    let mut disks = Vec::new();

    for line in df_output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 6 {
            // Skip pseudo filesystems
            let filesystem = parts[0];
            let mount_point = parts[5];

            // Only include real filesystems
            if filesystem.starts_with('/') || filesystem.starts_with("tmpfs") || filesystem.starts_with("/dev") {
                // Skip tmpfs, devtmpfs, etc. but keep /dev/* partitions
                if mount_point == "/" || mount_point.starts_with("/home") || mount_point.starts_with("/var") || mount_point.starts_with("/mnt") || mount_point.starts_with("/data") {
                    let total: u64 = parts[1].parse().unwrap_or(0) * 1024; // Convert 1K blocks to bytes
                    let used: u64 = parts[2].parse().unwrap_or(0) * 1024;
                    let available: u64 = parts[3].parse().unwrap_or(0) * 1024;

                    let usage_percent = if total > 0 {
                        (used as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };

                    disks.push(DiskInfo {
                        mount_point: mount_point.to_string(),
                        filesystem: filesystem.to_string(),
                        total,
                        used,
                        available,
                        usage_percent,
                    });
                }
            }
        }
    }

    // If no disks were found with strict filtering, try to get at least root
    if disks.is_empty() {
        for line in df_output.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 && parts[5] == "/" {
                let total: u64 = parts[1].parse().unwrap_or(0) * 1024;
                let used: u64 = parts[2].parse().unwrap_or(0) * 1024;
                let available: u64 = parts[3].parse().unwrap_or(0) * 1024;

                let usage_percent = if total > 0 {
                    (used as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                disks.push(DiskInfo {
                    mount_point: "/".to_string(),
                    filesystem: parts[0].to_string(),
                    total,
                    used,
                    available,
                    usage_percent,
                });
                break;
            }
        }
    }

    disks
}

/// Parse the output of /proc/net/dev
fn parse_network_info(netdev_output: &str) -> Vec<NetworkInfo> {
    let mut interfaces = Vec::new();

    for line in netdev_output.lines().skip(2) {
        let line = line.trim();
        if let Some(colon_pos) = line.find(':') {
            let interface = line[..colon_pos].trim();
            let stats = line[colon_pos + 1..].trim();
            let parts: Vec<&str> = stats.split_whitespace().collect();

            // Skip loopback interface
            if interface == "lo" {
                continue;
            }

            if parts.len() >= 10 {
                interfaces.push(NetworkInfo {
                    interface: interface.to_string(),
                    rx_bytes: parts[0].parse().unwrap_or(0),
                    rx_packets: parts[1].parse().unwrap_or(0),
                    tx_bytes: parts[8].parse().unwrap_or(0),
                    tx_packets: parts[9].parse().unwrap_or(0),
                });
            }
        }
    }

    interfaces
}

/// Parse uptime from /proc/uptime
fn parse_uptime(uptime_output: &str) -> u64 {
    let parts: Vec<&str> = uptime_output.split_whitespace().collect();
    if !parts.is_empty() {
        parts[0].parse::<f64>().unwrap_or(0.0) as u64
    } else {
        0
    }
}

fn default_cpu_info() -> CpuInfo {
    CpuInfo {
        usage_percent: 0.0,
        core_count: 1,
        load_average: [0.0, 0.0, 0.0],
    }
}

fn default_memory_info() -> MemoryInfo {
    MemoryInfo {
        total: 0,
        used: 0,
        free: 0,
        available: 0,
        usage_percent: 0.0,
        swap_total: 0,
        swap_used: 0,
    }
}

fn run_command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_load_average_macos(vm_loadavg_output: &str) -> [f64; 3] {
    // macOS `sysctl -n vm.loadavg` output example: { 1.20 1.35 1.42 }
    let normalized = vm_loadavg_output
        .replace(['{', '}'], "")
        .trim()
        .to_string();

    let parts: Vec<&str> = normalized.split_whitespace().collect();
    let mut load = [0.0, 0.0, 0.0];
    if parts.len() >= 3 {
        load[0] = parts[0].parse().unwrap_or(0.0);
        load[1] = parts[1].parse().unwrap_or(0.0);
        load[2] = parts[2].parse().unwrap_or(0.0);
    }

    load
}

fn parse_memory_info_macos(vm_stat_output: &str, memsize_output: &str) -> MemoryInfo {
    let total = memsize_output.trim().parse::<u64>().unwrap_or(0);

    let mut page_size = 4096u64;
    let mut free_pages = 0u64;
    let mut inactive_pages = 0u64;
    let mut speculative_pages = 0u64;

    for raw_line in vm_stat_output.lines() {
        let line = raw_line.trim();

        if let Some(value) = line.strip_prefix("Mach Virtual Memory Statistics: (page size of ") {
            if let Some(bytes_text) = value.strip_suffix(" bytes)") {
                page_size = bytes_text.parse::<u64>().unwrap_or(4096);
            }
            continue;
        }

        let parse_pages = |line: &str, key: &str| -> Option<u64> {
            let value = line.strip_prefix(key)?;
            let cleaned = value.trim().trim_end_matches('.').replace('.', "");
            cleaned.parse::<u64>().ok()
        };

        if let Some(v) = parse_pages(line, "Pages free:") {
            free_pages = v;
        } else if let Some(v) = parse_pages(line, "Pages inactive:") {
            inactive_pages = v;
        } else if let Some(v) = parse_pages(line, "Pages speculative:") {
            speculative_pages = v;
        }
    }

    let free = free_pages.saturating_mul(page_size);
    let available = free_pages
        .saturating_add(inactive_pages)
        .saturating_add(speculative_pages)
        .saturating_mul(page_size);
    let used = total.saturating_sub(available);

    let usage_percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    MemoryInfo {
        total,
        used,
        free,
        available,
        usage_percent,
        swap_total: 0,
        swap_used: 0,
    }
}

fn parse_wmic_value(output: &str, key: &str) -> Option<u64> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix(&format!("{}=", key)) {
            if let Ok(parsed) = value.trim().parse::<u64>() {
                return Some(parsed);
            }
        }
    }
    None
}

fn parse_disk_info_windows(wmic_output: &str) -> Vec<DiskInfo> {
    let mut disks = Vec::new();

    for line in wmic_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Node,") {
            continue;
        }

        let parts: Vec<&str> = trimmed.split(',').collect();
        if parts.len() < 5 {
            continue;
        }

        let filesystem = parts[2].trim().to_string();
        let mount_point = parts[1].trim().to_string();
        let free = parts[3].trim().parse::<u64>().unwrap_or(0);
        let total = parts[4].trim().parse::<u64>().unwrap_or(0);
        let used = total.saturating_sub(free);
        let usage_percent = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        disks.push(DiskInfo {
            mount_point,
            filesystem,
            total,
            used,
            available: free,
            usage_percent,
        });
    }

    disks
}

fn collect_local_server_status() -> ServerStatus {
    let os = std::env::consts::OS;

    let hostname = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| run_command_output("hostname", &[]).map(|s| s.trim().to_string()))
        .unwrap_or_else(|| "localhost".to_string());

    let core_count = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);

    let mut cpu = default_cpu_info();
    cpu.core_count = core_count;

    let mut memory = default_memory_info();
    let mut disks: Vec<DiskInfo> = Vec::new();
    let network: Vec<NetworkInfo> = Vec::new();

    // Local session data collection is OS-specific:
    // - Linux: prefer /proc-based metrics for consistency with remote SSH Linux collection.
    // - macOS: use `sysctl` and `vm_stat` because /proc is unavailable.
    // - Windows: use WMIC CSV/value output where available; otherwise gracefully degrade.
    match os {
        "linux" => {
            if let Ok(loadavg) = std::fs::read_to_string("/proc/loadavg") {
                cpu.load_average = parse_load_average(&loadavg);
            }

            if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
                memory = parse_memory_info(&meminfo);
            }

            if let Some(df_output) = run_command_output("df", &["-P"]) {
                disks = parse_disk_info(&df_output);
            }
        }
        "macos" => {
            if let Some(loadavg) = run_command_output("sysctl", &["-n", "vm.loadavg"]) {
                cpu.load_average = parse_load_average_macos(&loadavg);
            }

            let vm_stat = run_command_output("vm_stat", &[]).unwrap_or_default();
            let memsize = run_command_output("sysctl", &["-n", "hw.memsize"]).unwrap_or_default();
            if !vm_stat.is_empty() || !memsize.trim().is_empty() {
                memory = parse_memory_info_macos(&vm_stat, &memsize);
            }

            if let Some(df_output) = run_command_output("df", &["-P"]) {
                disks = parse_disk_info(&df_output);
            }
        }
        "windows" => {
            if let Some(mem_output) = run_command_output(
                "wmic",
                &["OS", "get", "FreePhysicalMemory,TotalVisibleMemorySize", "/value"],
            ) {
                let free_kb = parse_wmic_value(&mem_output, "FreePhysicalMemory").unwrap_or(0);
                let total_kb = parse_wmic_value(&mem_output, "TotalVisibleMemorySize").unwrap_or(0);
                let free = free_kb.saturating_mul(1024);
                let total = total_kb.saturating_mul(1024);
                let used = total.saturating_sub(free);
                let usage_percent = if total > 0 {
                    (used as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                memory = MemoryInfo {
                    total,
                    used,
                    free,
                    available: free,
                    usage_percent,
                    swap_total: 0,
                    swap_used: 0,
                };
            }

            if let Some(disk_output) = run_command_output(
                "wmic",
                &["logicaldisk", "get", "DeviceID,FileSystem,FreeSpace,Size", "/format:csv"],
            ) {
                disks = parse_disk_info_windows(&disk_output);
            }
        }
        _ => {}
    }

    ServerStatus {
        hostname,
        uptime_seconds: 0,
        cpu,
        memory,
        disks,
        network,
        collected_at: chrono::Utc::now().timestamp(),
    }
}

/// Get server status metrics.
/// - Local session: collect from host OS without SSH exec, with OS-specific fallbacks.
/// - Remote SSH session: keep existing Linux /proc-based collection through SSH exec.
#[tauri::command]
pub async fn get_server_status(
    manager: State<'_, Arc<SessionManager>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    request: SessionIdRequest,
) -> Result<ServerStatus, String> {
    // Local shell sessions are not SSH-backed; collect metrics directly from this machine.
    if local_shell_manager.get_session(&request.session_id).await.is_some() {
        return Ok(collect_local_server_status());
    }

    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    // Remote SSH logic remains Linux-oriented and unchanged.
    let combined_cmd = r#"
echo "===HOSTNAME==="; hostname;
echo "===UPTIME==="; cat /proc/uptime;
echo "===LOADAVG==="; cat /proc/loadavg;
echo "===CPUCOUNT==="; nproc;
echo "===CPUSTAT==="; head -1 /proc/stat;
echo "===MEMINFO==="; cat /proc/meminfo;
echo "===DISKINFO==="; df -P;
echo "===NETDEV==="; cat /proc/net/dev
"#;

    let output = session
        .exec_command(combined_cmd)
        .await
        .map_err(|e| format!("Failed to execute status command: {}", e))?;

    // Parse the combined output
    let mut hostname = String::from("unknown");
    let mut uptime_seconds = 0u64;
    let mut load_average = [0.0, 0.0, 0.0];
    let mut cpu_count = 1u32;
    let mut cpu_stat = String::new();
    let mut meminfo = String::new();
    let mut df_output = String::new();
    let mut netdev = String::new();

    let mut current_section = "";
    let mut section_buffer = String::new();

    for line in output.lines() {
        if line.starts_with("===") && line.ends_with("===") {
            // Save previous section
            match current_section {
                "HOSTNAME" => hostname = section_buffer.trim().to_string(),
                "UPTIME" => uptime_seconds = parse_uptime(&section_buffer),
                "LOADAVG" => load_average = parse_load_average(&section_buffer),
                "CPUCOUNT" => cpu_count = section_buffer.trim().parse().unwrap_or(1),
                "CPUSTAT" => cpu_stat = section_buffer.clone(),
                "MEMINFO" => meminfo = section_buffer.clone(),
                "DISKINFO" => df_output = section_buffer.clone(),
                "NETDEV" => netdev = section_buffer.clone(),
                _ => {}
            }

            // Start new section
            current_section = line.trim_matches('=');
            section_buffer.clear();
        } else {
            section_buffer.push_str(line);
            section_buffer.push('\n');
        }
    }

    // Don't forget the last section
    match current_section {
        "HOSTNAME" => hostname = section_buffer.trim().to_string(),
        "UPTIME" => uptime_seconds = parse_uptime(&section_buffer),
        "LOADAVG" => load_average = parse_load_average(&section_buffer),
        "CPUCOUNT" => cpu_count = section_buffer.trim().parse().unwrap_or(1),
        "CPUSTAT" => cpu_stat = section_buffer.clone(),
        "MEMINFO" => meminfo = section_buffer.clone(),
        "DISKINFO" => df_output = section_buffer.clone(),
        "NETDEV" => netdev = section_buffer.clone(),
        _ => {}
    }

    // Parse CPU info
    let mut cpu = parse_cpu_usage(&cpu_stat, &cpu_count.to_string());
    cpu.load_average = load_average;
    cpu.core_count = cpu_count;

    // Parse memory info
    let memory = parse_memory_info(&meminfo);

    // Parse disk info
    let disks = parse_disk_info(&df_output);

    // Parse network info
    let network = parse_network_info(&netdev);

    Ok(ServerStatus {
        hostname,
        uptime_seconds,
        cpu,
        memory,
        disks,
        network,
        collected_at: chrono::Utc::now().timestamp(),
    })
}
