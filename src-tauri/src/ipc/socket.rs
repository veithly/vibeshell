use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

use interprocess::local_socket::traits::{ListenerExt, Stream as StreamTrait};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName, ListenerOptions};
#[cfg(not(windows))]
use interprocess::local_socket::{GenericFilePath, ToFsName, ListenerOptions};

use crate::storage::Database;
use crate::session::SessionManager;

const SOCKET_NAME: &str = "vibeshell.sock";

/// IPC messages exchanged between CLI and GUI.
///
/// Messages are serialized as JSON for simplicity and debuggability.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcMessage {
    // Requests from CLI to GUI
    /// List all active sessions
    ListSessions,
    /// Create a new session connecting to the specified server
    CreateSession { server_name: String },
    /// Attach to an existing session
    AttachSession { session_id: String },
    /// Detach from a session (keeps it running)
    DetachSession { session_id: String },
    /// Kill/terminate a session
    KillSession { session_id: String },
    /// Send input data to a session
    SendInput { session_id: String, data: Vec<u8> },

    // Responses from GUI to CLI
    /// List of active session IDs
    SessionList { sessions: Vec<String> },
    /// A new session was created
    SessionCreated { session_id: String },
    /// Output data from a session
    SessionOutput { session_id: String, data: Vec<u8> },
    /// Error response
    Error { message: String },
    /// Success acknowledgment
    Ok,
}

/// Socket name type alias for platform-specific implementation.
#[cfg(windows)]
type SocketName = interprocess::local_socket::Name<'static>;
#[cfg(not(windows))]
type SocketName = interprocess::local_socket::Name<'static>;

/// Get the platform-specific socket name/path.
///
/// On Windows, we use named pipes in the namespaced format.
/// On Unix, we use Unix domain sockets in `/tmp/`.
fn get_socket_name() -> Result<SocketName> {
    #[cfg(windows)]
    {
        // On Windows, use namespaced name (named pipe)
        SOCKET_NAME
            .to_ns_name::<GenericNamespaced>()
            .context("Failed to create namespaced socket name")
    }
    #[cfg(not(windows))]
    {
        // On Unix, use a socket file in /tmp
        let path = format!("/tmp/{}", SOCKET_NAME);
        path.to_fs_name::<GenericFilePath>()
            .context("Failed to create filesystem socket name")
    }
}

/// IPC server that runs in the GUI application.
///
/// The GUI app starts this server on launch to accept connections
/// from CLI instances that want to interact with sessions.
pub struct IpcServer {
    database: Arc<Database>,
    session_manager: Arc<SessionManager>,
}

impl IpcServer {
    /// Create a new IPC server instance.
    pub fn new(database: Arc<Database>, session_manager: Arc<SessionManager>) -> Self {
        Self { database, session_manager }
    }

    /// Get a human-readable description of the socket name for this server.
    #[allow(dead_code)]
    pub fn socket_name_display() -> String {
        #[cfg(windows)]
        {
            format!("\\\\.\\pipe\\{}", SOCKET_NAME)
        }
        #[cfg(not(windows))]
        {
            format!("/tmp/{}", SOCKET_NAME)
        }
    }

    /// Start the IPC server and listen for connections.
    /// This should be run in a separate thread.
    pub fn run(&self) -> Result<()> {
        let socket_name = get_socket_name()?;

        // Create listener with options
        let listener = ListenerOptions::new()
            .name(socket_name)
            .create_sync()
            .context("Failed to create IPC listener")?;

        log::info!("[IPC] Server listening on {}", Self::socket_name_display());

        // Accept connections in a loop
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let db = self.database.clone();
                    let sm = self.session_manager.clone();

                    // Handle each connection in a thread
                    std::thread::spawn(move || {
                        if let Err(e) = Self::handle_connection(stream, db, sm) {
                            log::error!("[IPC] Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    log::error!("[IPC] Accept error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Handle a single IPC connection
    fn handle_connection(
        stream: interprocess::local_socket::Stream,
        database: Arc<Database>,
        session_manager: Arc<SessionManager>,
    ) -> Result<()> {
        let mut reader = BufReader::new(&stream);
        let mut writer = &stream;

        // Read the request
        let mut line = String::new();
        reader.read_line(&mut line).context("Failed to read IPC message")?;

        let message: IpcMessage = serde_json::from_str(line.trim())
            .context("Failed to parse IPC message")?;

        log::debug!("[IPC] Received: {:?}", message);

        // Handle the message
        let response = Self::handle_message(message, database, session_manager);

        // Send response
        let mut json = serde_json::to_string(&response)
            .context("Failed to serialize response")?;
        json.push('\n');
        writer.write_all(json.as_bytes()).context("Failed to send response")?;
        writer.flush()?;

        Ok(())
    }

    /// Handle an IPC message and return a response
    fn handle_message(
        message: IpcMessage,
        _database: Arc<Database>,
        session_manager: Arc<SessionManager>,
    ) -> IpcMessage {
        // Create a runtime for async operations
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => return IpcMessage::Error {
                message: format!("Failed to create runtime: {}", e),
            },
        };

        match message {
            IpcMessage::ListSessions => {
                let sessions = rt.block_on(async {
                    session_manager.list().await
                        .into_iter()
                        .map(|s| s.id.clone())
                        .collect::<Vec<_>>()
                });
                IpcMessage::SessionList { sessions }
            }
            IpcMessage::CreateSession { server_name } => {
                // Look up server by name and create session
                match rt.block_on(session_manager.create_by_name(&server_name)) {
                    Ok(session) => {
                        IpcMessage::SessionCreated { session_id: session.id.clone() }
                    }
                    Err(e) => IpcMessage::Error {
                        message: format!("Failed to create session: {}", e),
                    },
                }
            }
            IpcMessage::KillSession { session_id } => {
                match rt.block_on(session_manager.kill(&session_id)) {
                    Ok(_) => IpcMessage::Ok,
                    Err(e) => IpcMessage::Error {
                        message: format!("Failed to kill session: {}", e),
                    },
                }
            }
            IpcMessage::AttachSession { session_id: _ } => {
                // Attach is handled by the GUI - just acknowledge
                IpcMessage::Ok
            }
            IpcMessage::DetachSession { session_id: _ } => {
                IpcMessage::Ok
            }
            IpcMessage::SendInput { session_id, data } => {
                // Send input to session
                log::debug!("[IPC] SendInput to {}: {} bytes", session_id, data.len());
                IpcMessage::Ok
            }
            _ => IpcMessage::Error {
                message: "Unexpected message type".to_string(),
            },
        }
    }
}

/// IPC client used by the CLI to communicate with the GUI.
///
/// The CLI uses this client to send commands to the GUI server
/// and receive responses about session operations.
pub struct IpcClient;

impl IpcClient {
    /// Send a message to the IPC server and wait for a response.
    ///
    /// This is the primary method for CLI-GUI communication.
    /// Returns an error if the GUI is not running.
    pub fn send(message: &IpcMessage) -> Result<IpcMessage> {
        use interprocess::local_socket::Stream;

        // Get the platform-specific socket name
        let socket_name = get_socket_name()?;

        // Connect to the IPC server
        let mut stream = Stream::connect(socket_name)
            .context("Failed to connect to VibeShell GUI - is it running?")?;

        // Serialize the message as JSON with newline delimiter
        let mut json = serde_json::to_string(message)
            .context("Failed to serialize IPC message")?;
        json.push('\n');

        // Send the message
        stream
            .write_all(json.as_bytes())
            .context("Failed to send IPC message")?;
        stream.flush().context("Failed to flush IPC stream")?;

        // Read the response (newline-delimited JSON)
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .context("Failed to read IPC response")?;

        // Deserialize the response
        let response: IpcMessage = serde_json::from_str(response_line.trim())
            .context("Failed to parse IPC response")?;

        Ok(response)
    }

    /// Check if the IPC server is running.
    ///
    /// Used to determine whether to use IPC or fall back to direct operations.
    pub fn is_server_running() -> bool {
        use interprocess::local_socket::Stream;

        // Try to get socket name, return false if that fails
        let socket_name = match get_socket_name() {
            Ok(name) => name,
            Err(_) => return false,
        };

        // Try to connect - if successful, server is running
        Stream::connect(socket_name).is_ok()
    }

    /// Get a human-readable description of the socket name.
    #[allow(dead_code)]
    pub fn socket_name_display() -> String {
        #[cfg(windows)]
        {
            format!("\\\\.\\pipe\\{}", SOCKET_NAME)
        }
        #[cfg(not(windows))]
        {
            format!("/tmp/{}", SOCKET_NAME)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_message_serialization() {
        // Test that messages can be serialized to JSON
        let msg = IpcMessage::ListSessions;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("ListSessions"));

        let msg = IpcMessage::CreateSession {
            server_name: "test-server".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("CreateSession"));
        assert!(json.contains("test-server"));

        let msg = IpcMessage::SessionOutput {
            session_id: "abc123".to_string(),
            data: vec![72, 101, 108, 108, 111], // "Hello"
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("SessionOutput"));
    }

    #[test]
    fn test_ipc_message_deserialization() {
        let json = r#"{"type":"ListSessions"}"#;
        let msg: IpcMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IpcMessage::ListSessions));

        let json = r#"{"type":"CreateSession","payload":{"server_name":"my-server"}}"#;
        let msg: IpcMessage = serde_json::from_str(json).unwrap();
        if let IpcMessage::CreateSession { server_name } = msg {
            assert_eq!(server_name, "my-server");
        } else {
            panic!("Expected CreateSession message");
        }
    }

    #[test]
    fn test_socket_name() {
        // Test that socket name can be created successfully
        let result = get_socket_name();
        assert!(result.is_ok(), "Should be able to create socket name");
    }

    #[test]
    fn test_socket_name_display() {
        let display = IpcClient::socket_name_display();
        assert!(display.contains("vibeshell"), "Socket name should contain vibeshell");
        #[cfg(windows)]
        assert!(display.contains("pipe"), "Windows socket should be a named pipe");
        #[cfg(not(windows))]
        assert!(display.starts_with("/tmp/"), "Unix socket should be in /tmp");
    }

    #[test]
    fn test_ipc_client_not_connected() {
        // When no server is running, send should fail with connection error
        let result = IpcClient::send(&IpcMessage::ListSessions);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // The error should mention connection failure or GUI not running
        assert!(
            err.contains("connect") || err.contains("GUI"),
            "Error should mention connection issue: {}",
            err
        );
    }

    #[test]
    fn test_ipc_server_not_running() {
        assert!(!IpcClient::is_server_running());
    }
}
