//! End-to-end IPC test for interactive SSH.
//!
//! This test requires the VibeShell GUI to be running with a valid "SG" server configured.
//! Run with: cargo test -p vshell --test ipc_e2e_test -- --ignored --nocapture

use std::io::BufRead;
use std::time::Duration;

use vibeshell_core::ipc::{IpcClient, IpcMessage};

#[test]
#[ignore = "requires VibeShell GUI running with SG server configured"]
fn test_ssh_interactive_pipeline() {
    // 1. Verify GUI is running
    assert!(
        IpcClient::is_server_running(),
        "VibeShell GUI must be running for this test"
    );

    // 2. Create a session
    let response = IpcClient::send(&IpcMessage::CreateSession {
        server_name: "SG".to_string(),
    })
    .expect("Failed to create session");

    let session_id = match response {
        IpcMessage::SessionCreated { session_id } => {
            println!("Session created: {}", session_id);
            session_id
        }
        IpcMessage::Error { message } => {
            panic!("Failed to create session: {}", message);
        }
        other => {
            panic!("Unexpected response: {:?}", other);
        }
    };

    // 3. Attach to the session via streaming
    let mut reader = IpcClient::connect_streaming(&IpcMessage::AttachSession {
        session_id: session_id.clone(),
    })
    .expect("Failed to attach");

    // 4. Read the initial Ok response
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .expect("Failed to read attach response");
    let attach_response: IpcMessage =
        serde_json::from_str(first_line.trim()).expect("Failed to parse attach response");
    match attach_response {
        IpcMessage::Ok => println!("Attached successfully"),
        IpcMessage::Error { message } => panic!("Attach failed: {}", message),
        _ => panic!("Unexpected attach response: {:?}", attach_response),
    }

    // 5. Wait a moment for the SSH session to send its banner/prompt
    std::thread::sleep(Duration::from_secs(2));

    // 6. Send a simple command via SendInput
    // The expected marker must not appear in the echoed command itself.
    let cmd = "printf '%s%s\\n' VIBESHELL_ PIPELINE_OK\n";
    let send_result = IpcClient::send(&IpcMessage::SendInput {
        session_id: session_id.clone(),
        data: cmd.as_bytes().to_vec(),
    })
    .expect("Failed to send input");
    assert!(
        matches!(send_result, IpcMessage::Ok),
        "SendInput should return Ok, got: {:?}",
        send_result
    );
    println!("Sent harmless output-marker command");

    // 7. Read output — look for our test marker in the streaming output
    //    The streaming reader blocks on read_line(), so we use a thread with timeout.
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);

    let read_thread = std::thread::spawn(move || {
        let mut line_buf = String::new();
        let mut tail = Vec::new();
        let mut marker_found = false;
        loop {
            line_buf.clear();
            match reader.read_line(&mut line_buf) {
                Ok(0) => {
                    println!("EOF from streaming connection");
                    break;
                }
                Ok(_) => {
                    let trimmed = line_buf.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<IpcMessage>(trimmed) {
                        Ok(IpcMessage::SessionOutput { data, .. }) => {
                            const MARKER: &[u8] = b"VIBESHELL_PIPELINE_OK";
                            tail.extend_from_slice(&data);
                            if tail.windows(MARKER.len()).any(|bytes| bytes == MARKER) {
                                marker_found = true;
                                break;
                            }
                            let keep_from = tail.len().saturating_sub(MARKER.len() - 1);
                            tail.drain(..keep_from);
                        }
                        Ok(IpcMessage::SessionEnded { reason }) => {
                            println!("[SESSION ENDED] {}", reason);
                            break;
                        }
                        Ok(other) => {
                            println!("[OTHER] {:?}", other);
                        }
                        Err(e) => {
                            println!("[PARSE ERROR] {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("[READ ERROR] {}", e);
                    break;
                }
            }
        }
        let _ = finished_tx.send(marker_found);
    });

    // join() itself has no deadline. Bound the observation before cleanup,
    // rather than leaving the test hung when the server sends no output.
    let marker_found = finished_rx
        .recv_timeout(Duration::from_secs(20))
        .unwrap_or(false);

    // 8. Kill the session
    let kill_result = IpcClient::send(&IpcMessage::KillSession {
        session_id: session_id.clone(),
    });
    println!("Kill result: {:?}", kill_result);
    if read_thread.is_finished() {
        read_thread.join().expect("output reader panicked");
    }

    // 9. Assert we found the marker
    assert!(
        marker_found,
        "Expected actual VIBESHELL_PIPELINE_OK command output within 20 seconds"
    );

    println!("SUCCESS: Full interactive SSH pipeline works!");
}
