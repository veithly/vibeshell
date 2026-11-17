//! Isolated tsh-shaped subprocess + real SFTP wire protocol. These tests never
//! log in to Teleport, inspect user profiles, or touch the production database.
use super::*;
use crate::session::{Session, SessionState};
use crate::sftp::{
    build_directory_transfer_plan, helpers::*, transfer_directory_to_sftp, DirectoryTransferMode,
    DirectoryTransferOptions,
};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode,
};
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};
use tokio::{
    io::AsyncReadExt,
    net::TcpListener,
    sync::{broadcast, mpsc},
};

#[derive(Clone)]
enum Node {
    Dir,
    File(Vec<u8>),
}
#[derive(Clone)]
enum OpenHandle {
    File(String),
    Dir(String, bool),
}
struct MemoryFs {
    nodes: HashMap<String, Node>,
    handles: HashMap<String, OpenHandle>,
}
struct Handler(Arc<Mutex<MemoryFs>>);

fn attrs(node: &Node) -> FileAttributes {
    FileAttributes {
        size: Some(match node {
            Node::Dir => 0,
            Node::File(bytes) => bytes.len() as u64,
        }),
        permissions: Some(match node {
            Node::Dir => 0o040755,
            Node::File(_) => 0o100644,
        }),
        ..Default::default()
    }
}
fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: String::new(),
        language_tag: "en".into(),
    }
}
fn parent(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(p, _)| if p.is_empty() { "/" } else { p })
        .unwrap_or("/")
}

impl russh_sftp::server::Handler for Handler {
    type Error = StatusCode;
    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }
    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
        let path = if path == "." || path.is_empty() {
            "/".into()
        } else {
            path
        };
        Ok(Name {
            id,
            files: vec![File::new(path, attrs(&Node::Dir))],
        })
    }
    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let fs = self.0.lock().unwrap();
        let node = fs.nodes.get(&path).ok_or(StatusCode::NoSuchFile)?;
        Ok(Attrs {
            id,
            attrs: attrs(node),
        })
    }
    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        self.stat(id, path).await
    }
    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, StatusCode> {
        let path = match self.0.lock().unwrap().handles.get(&handle) {
            Some(OpenHandle::File(path)) | Some(OpenHandle::Dir(path, _)) => path.clone(),
            None => return Err(StatusCode::Failure),
        };
        self.stat(id, path).await
    }
    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        _: FileAttributes,
    ) -> Result<Handle, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        if !matches!(fs.nodes.get(parent(&filename)), Some(Node::Dir)) {
            return Err(StatusCode::NoSuchFile);
        }
        if matches!(fs.nodes.get(&filename), Some(Node::Dir)) {
            return Err(StatusCode::Failure);
        }
        if flags.contains(OpenFlags::EXCLUDE) && fs.nodes.contains_key(&filename) {
            return Err(StatusCode::Failure);
        }
        if flags.contains(OpenFlags::CREATE) {
            fs.nodes
                .entry(filename.clone())
                .or_insert(Node::File(Vec::new()));
        }
        if !fs.nodes.contains_key(&filename) {
            return Err(StatusCode::NoSuchFile);
        }
        if flags.contains(OpenFlags::TRUNCATE) {
            fs.nodes.insert(filename.clone(), Node::File(Vec::new()));
        }
        let handle = format!("file-{id}");
        fs.handles
            .insert(handle.clone(), OpenHandle::File(filename));
        Ok(Handle { id, handle })
    }
    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, StatusCode> {
        let fs = self.0.lock().unwrap();
        let Some(OpenHandle::File(path)) = fs.handles.get(&handle) else {
            return Err(StatusCode::Failure);
        };
        let Some(Node::File(bytes)) = fs.nodes.get(path) else {
            return Err(StatusCode::NoSuchFile);
        };
        let start = offset as usize;
        if start >= bytes.len() {
            return Err(StatusCode::Eof);
        }
        Ok(Data {
            id,
            data: bytes[start..bytes.len().min(start + len as usize)].to_vec(),
        })
    }
    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        let Some(OpenHandle::File(path)) = fs.handles.get(&handle).cloned() else {
            return Err(StatusCode::Failure);
        };
        let Some(Node::File(bytes)) = fs.nodes.get_mut(&path) else {
            return Err(StatusCode::NoSuchFile);
        };
        let offset = offset as usize;
        bytes.resize(bytes.len().max(offset + data.len()), 0);
        bytes[offset..offset + data.len()].copy_from_slice(&data);
        Ok(ok(id))
    }
    async fn close(&mut self, id: u32, handle: String) -> Result<Status, StatusCode> {
        self.0.lock().unwrap().handles.remove(&handle);
        Ok(ok(id))
    }
    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _: FileAttributes,
    ) -> Result<Status, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        if fs.nodes.contains_key(&path) {
            return Err(StatusCode::Failure);
        }
        if !matches!(fs.nodes.get(parent(&path)), Some(Node::Dir)) {
            return Err(StatusCode::NoSuchFile);
        }
        fs.nodes.insert(path, Node::Dir);
        Ok(ok(id))
    }
    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        if !matches!(fs.nodes.get(&path), Some(Node::Dir)) {
            return Err(StatusCode::NoSuchFile);
        }
        let handle = format!("dir-{id}");
        fs.handles
            .insert(handle.clone(), OpenHandle::Dir(path, false));
        Ok(Handle { id, handle })
    }
    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        let path = match fs.handles.get_mut(&handle) {
            Some(OpenHandle::Dir(path, read)) if !*read => {
                *read = true;
                path.clone()
            }
            Some(OpenHandle::Dir(_, _)) => return Err(StatusCode::Eof),
            _ => return Err(StatusCode::Failure),
        };
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let files = fs
            .nodes
            .iter()
            .filter_map(|(name, node)| {
                let component = name.strip_prefix(&prefix)?;
                if component.is_empty() || component.contains('/') {
                    return None;
                }
                Some(File::new(component, attrs(node)))
            })
            .collect();
        Ok(Name { id, files })
    }
    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        if !matches!(fs.nodes.get(&filename), Some(Node::File(_))) {
            return Err(StatusCode::NoSuchFile);
        }
        fs.nodes.remove(&filename);
        Ok(ok(id))
    }
    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        let mut fs = self.0.lock().unwrap();
        let prefix = format!("{path}/");
        if !matches!(fs.nodes.get(&path), Some(Node::Dir))
            || fs.nodes.keys().any(|p| p.starts_with(&prefix))
        {
            return Err(StatusCode::Failure);
        }
        fs.nodes.remove(&path);
        Ok(ok(id))
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    target: TeleportTarget,
    listener: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let fs = Arc::new(Mutex::new(MemoryFs {
            nodes: HashMap::from([("/".into(), Node::Dir)]),
            handles: HashMap::new(),
        }));
        let listener = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                russh_sftp::server::run(stream, Handler(fs.clone())).await;
            }
        });
        let script = directory.path().join("fake tsh client");
        let program = format!(
            r##"#!/usr/bin/env python3
import os, re, socket, sys, threading
args = sys.argv[1:]
assert args[:3] == ['--proxy', 'fixture.invalid', 'ssh']
assert '--no-relogin' in args and '--request-mode=off' in args
assert '--insecure' not in args
node = args[args.index('--') + 1]
command = args[-1]
if node.endswith('@denied'):
    sys.stderr.write('fixture authentication denied')
    sys.exit(7)
if 'sftp-server' in command:
    assert '--no-tty' in args
    connection = socket.create_connection(('127.0.0.1', {port}))
    sys.stderr.write('fixture diagnostic, not protocol bytes\n')
    sys.stderr.flush()
    def send():
        try:
            while True:
                data = os.read(0, 65536)
                if not data: break
                connection.sendall(data)
        finally:
            try: connection.shutdown(socket.SHUT_WR)
            except OSError: pass
    threading.Thread(target=send, daemon=True).start()
    while True:
        data = connection.recv(65536)
        if not data: break
        sys.stdout.buffer.write(data)
        sys.stdout.buffer.flush()
    sys.exit(0)
os.chdir(os.path.dirname(__file__))
os.environ['HOME'] = os.path.dirname(__file__)
os.environ['ENV'] = ''
os.environ['SHELL'] = '/bin/sh'
if 'vibeshell-ready;' in command:
    assert '--tty' in args
    nonce = re.search('vibeshell-ready;([a-f0-9]+)', command).group(1)
    sys.stdout.buffer.write(('\x1b]999;vibeshell-ready;' + nonce + '\x07').encode())
    sys.stdout.flush()
    os.execv('/bin/sh', ['sh', '-i'])
os.execv('/bin/sh', ['sh', '-c', command])
"##
        );
        std::fs::write(&script, program).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut target = TeleportTarget::new("fixture.invalid", "fixture", "node").unwrap();
        target.binary = script.to_str().unwrap().into();
        Self {
            directory,
            target,
            listener,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.target.cancel();
        self.listener.abort();
    }
}

async fn read_file(sftp: &russh_sftp::client::SftpSession, path: &str) -> Vec<u8> {
    let mut file = sftp.open(path).await.unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.unwrap();
    bytes
}

#[tokio::test]
async fn subprocess_sftp_preserves_binary_names_exclusive_writes_and_real_sync_counts() {
    let fixture = Fixture::new().await;
    let sftp = fixture.target.open_sftp().await.unwrap();
    let path = "/ 空白 文 件\n.bin ";
    let bytes: Vec<u8> = (0..=255).cycle().take(600_001).collect();
    let exclusive = WriteRemoteFileOptions {
        create_parent_dirs: true,
        overwrite: false,
    };
    write_remote_file_with_options(&sftp, path, &bytes, exclusive)
        .await
        .unwrap();
    assert!(
        write_remote_file_with_options(&sftp, path, b"must not overwrite", exclusive)
            .await
            .is_err()
    );
    assert_eq!(read_file(&sftp, path).await, bytes);
    assert_eq!(
        sftp.metadata(path).await.unwrap().size,
        Some(bytes.len() as u64)
    );
    let entries: Vec<_> = sftp.read_dir("/").await.unwrap().collect();
    assert!(entries
        .iter()
        .any(|entry| entry.file_name() == path[1..] && !entry.file_type().is_dir()));

    let source = fixture.directory.path().join("source");
    std::fs::create_dir_all(source.join("sub")).unwrap();
    std::fs::write(source.join(".gitignore"), "cache/\n").unwrap();
    std::fs::write(source.join("sub/.gitignore"), "*.tmp\n").unwrap();
    std::fs::write(source.join("sub/ 数据.bin "), &bytes).unwrap();
    let root = "/sync destination ";
    for relative in ["cache/keep", "sub/ignored.tmp", "excluded.log", "stale.txt"] {
        write_remote_file(&sftp, &format!("{root}/{relative}"), b"keep unless extra")
            .await
            .unwrap();
    }
    let options = DirectoryTransferOptions {
        excluded_paths: vec!["*.log".into()],
        respect_gitignore: true,
        delete_extra: true,
    };
    let plan = build_directory_transfer_plan(&source, &options).unwrap();
    let first =
        transfer_directory_to_sftp(&sftp, &source, root, DirectoryTransferMode::Sync, &options)
            .await
            .unwrap();
    assert_eq!(first.remote_root, root);
    assert_eq!(first.uploaded_files, plan.files.len());
    assert_eq!(first.transferred_bytes, plan.total_bytes);
    assert_eq!(first.deleted_entries, 1);
    for relative in ["cache/keep", "sub/ignored.tmp", "excluded.log"] {
        assert_eq!(
            read_file(&sftp, &format!("{root}/{relative}")).await,
            b"keep unless extra"
        );
    }
    assert!(sftp.metadata(format!("{root}/stale.txt")).await.is_err());
    assert!(sftp.metadata("/sync destination").await.is_err());
    let entries: Vec<_> = sftp.read_dir(root).await.unwrap().collect();
    assert!(entries
        .iter()
        .any(|entry| entry.file_name() == "sub" && entry.file_type().is_dir()));
    let second =
        transfer_directory_to_sftp(&sftp, &source, root, DirectoryTransferMode::Sync, &options)
            .await
            .unwrap();
    assert_eq!(second.uploaded_files, 0);
    assert_eq!(second.transferred_bytes, 0);
    assert_eq!(second.created_directories, 0);
    assert_eq!(second.deleted_entries, 0);
    assert_eq!(second.skipped_files, plan.files.len());
    fixture.target.cancel();
    // russh-sftp 2.1 keeps an already queued request until its configured
    // response deadline even after EOF. The tsh process is killed immediately;
    // an in-flight protocol request still has a bounded 15-second failure path.
    assert!(
        tokio::time::timeout(Duration::from_secs(18), sftp.metadata(path))
            .await
            .unwrap()
            .is_err()
    );
    assert!(fixture.target.open_sftp().await.is_err());
}

#[tokio::test]
async fn shared_session_routes_terminal_quick_exec_plugins_sftp_and_teardown() {
    let fixture = Fixture::new().await;
    let (input, _input_rx) = mpsc::channel(16);
    let (output, _) = broadcast::channel(16);
    let session = Arc::new(Session::new(
        "fixture-server".into(),
        "fixture".into(),
        input,
        output,
    ));
    let (tx, mut rx) = mpsc::channel(16);
    let pty = spawn_tsh_ssh(&fixture.target, 80, 24, tx).await.unwrap();
    session.set_teleport_target(fixture.target.clone()).await;
    session.set_teleport_pty(pty).await;
    session.set_state(SessionState::Connected).await;
    assert_eq!(session.get_state().await, SessionState::Connected);
    assert_eq!(
        session
            .exec_command_with_stdin("cat", Some("stdin roundtrip\n"))
            .await
            .unwrap(),
        "stdin roundtrip\n"
    );
    let quick = session
        .exec_quick_command("printf quick; exit 7")
        .await
        .unwrap();
    assert_eq!(quick.exit_code, 7);
    assert_eq!(quick.output, "quick");
    assert!(session
        .exec_command_with_stdin("exit 9", None)
        .await
        .unwrap_err()
        .to_string()
        .contains("9"));
    session.resize_pty(100, 30).await.unwrap();
    session
        .write_to_ssh(b"printf 'shared-terminal-output\\n'\n")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut output = Vec::new();
        loop {
            output.extend_from_slice(&rx.recv().await.unwrap());
            assert!(!String::from_utf8_lossy(&output).contains("vibeshell-ready;"));
            if String::from_utf8_lossy(&output).contains("shared-terminal-output\r\n") {
                break;
            }
        }
    })
    .await
    .unwrap();
    let sftp = session.open_sftp_session().await.unwrap();
    write_remote_file(&sftp, "/owned-by-session", b"ok")
        .await
        .unwrap();
    let running = fixture
        .target
        .exec("exec sleep 30", None, Duration::from_secs(40), 1024);
    tokio::pin!(running);
    tokio::select! { _ = &mut running => panic!("command unexpectedly ended"), _ = tokio::time::sleep(Duration::from_millis(100)) => {} }
    session.disconnect().await.unwrap();
    assert_eq!(session.get_state().await, SessionState::Disconnected);
    assert!(tokio::time::timeout(Duration::from_secs(3), running)
        .await
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    assert!(sftp.metadata("/owned-by-session").await.is_err());
    assert!(session.write_to_ssh(b"must not replay\n").await.is_err());
}

#[tokio::test]
async fn authentication_failure_never_produces_a_ready_session_or_sftp_success() {
    let mut fixture = Fixture::new().await;
    fixture.target.node = "denied".into();
    let (tx, _rx) = mpsc::channel(16);
    assert!(spawn_tsh_ssh(&fixture.target, 80, 24, tx).await.is_err());
    assert!(fixture.target.open_sftp().await.is_err());
}
