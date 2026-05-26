# Headless CLI SSH And SFTP Implementation Plan

> **For Codex:** Execute this plan task-by-task. Use parallel agents only for independent files/tasks.

**Goal:** Let `vshell` provide terminal-native SSH and SFTP workflows without depending on the Tauri UI process being open.

**Architecture:** Keep the current IPC/session model, but make it runnable in a headless daemon started by the CLI. Extend the IPC protocol with SFTP operations so terminal workflows can reuse the same background session manager and survive independently from the UI window lifecycle.

**Tech Stack:** Rust, Clap, Tokio, interprocess local sockets, russh, russh-sftp, SQLite via rusqlite

---

### Task 1: Headless daemon bootstrap

**Files:**
- Modify: `cli/src/main.rs`
- Create: `cli/src/daemon.rs`
- Test: `cli/src/main.rs`

**Step 1: Write the failing test**

Add a parser test that accepts `vshell daemon start`.

**Step 2: Run test to verify it fails**

Run: `cargo test -p vshell parses_daemon_start_command`
Expected: FAIL because the subcommand does not exist yet.

**Step 3: Write minimal implementation**

Add a `daemon` CLI subcommand with:
- `start` to spawn a detached background process
- `run` to run the IPC service in the foreground
- `status` to report whether the socket is reachable

Add helper logic that starts `vshell daemon start` automatically when IPC is unavailable for CLI SSH/SFTP/session commands.

**Step 4: Run test to verify it passes**

Run: `cargo test -p vshell parses_daemon_start_command`
Expected: PASS

**Step 5: Commit**

```bash
git add cli/src/main.rs cli/src/daemon.rs
git commit -m "feat: add headless vshell daemon"
```

### Task 2: CLI IPC bootstrap helpers

**Files:**
- Create: `cli/src/ipc_support.rs`
- Modify: `cli/src/commands/server.rs`
- Modify: `cli/src/commands/session.rs`
- Modify: `cli/src/commands/ssh.rs`
- Modify: `cli/src/terminal.rs`

**Step 1: Write the failing test**

Add unit coverage for helper behavior that reports a daemon startup timeout cleanly when the socket never appears.

**Step 2: Run test to verify it fails**

Run: `cargo test -p vshell`
Expected: FAIL because the helper module does not exist yet.

**Step 3: Write minimal implementation**

Introduce a shared helper that:
- checks IPC availability
- auto-starts the daemon when missing
- waits briefly for the socket to become ready
- returns a daemon-oriented error message instead of a GUI-oriented one

Update existing CLI commands to use the helper.

**Step 4: Run test to verify it passes**

Run: `cargo test -p vshell`
Expected: PASS for new helper tests.

**Step 5: Commit**

```bash
git add cli/src/ipc_support.rs cli/src/commands/server.rs cli/src/commands/session.rs cli/src/commands/ssh.rs cli/src/terminal.rs
git commit -m "refactor: auto-start headless ipc for cli"
```

### Task 3: IPC SFTP protocol

**Files:**
- Modify: `src-tauri/src/ipc/socket.rs`
- Modify: `src-tauri/src/commands/sftp.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Write the failing test**

Add serialization tests for new SFTP IPC request/response variants.

**Step 2: Run test to verify it fails**

Run: `cargo test -p vibeshell ipc`
Expected: FAIL because SFTP IPC variants are not defined.

**Step 3: Write minimal implementation**

Extend `IpcMessage` with enough SFTP operations for terminal workflows:
- initialize session context
- list directories
- print working directory
- read file
- upload
- download
- mkdir
- delete
- rename
- stat

Implement handlers inside the IPC server with a dedicated SFTP state map keyed by session ID.

**Step 4: Run test to verify it passes**

Run: `cargo test -p vibeshell ipc`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/ipc/socket.rs src-tauri/src/commands/sftp.rs src-tauri/src/lib.rs
git commit -m "feat: add sftp operations to ipc server"
```

### Task 4: Terminal SFTP command

**Files:**
- Create: `cli/src/commands/sftp.rs`
- Modify: `cli/src/commands/mod.rs`
- Modify: `cli/src/main.rs`

**Step 1: Write the failing test**

Add parser coverage for `vshell sftp my-server`.

**Step 2: Run test to verify it fails**

Run: `cargo test -p vshell parses_sftp_command`
Expected: FAIL because the command does not exist yet.

**Step 3: Write minimal implementation**

Implement an interactive terminal SFTP REPL that:
- creates an SSH session
- initializes SFTP over IPC
- supports `pwd`, `ls`, `cd`, `get`, `put`, `cat`, `mkdir`, `rm`, `mv`, `help`, `quit`
- cleans up the temporary session on exit

**Step 4: Run test to verify it passes**

Run: `cargo test -p vshell parses_sftp_command`
Expected: PASS

**Step 5: Commit**

```bash
git add cli/src/commands/sftp.rs cli/src/commands/mod.rs cli/src/main.rs
git commit -m "feat: add terminal sftp workflow"
```

### Task 5: Verification

**Files:**
- Modify: `README.md` (if CLI usage docs need updates)

**Step 1: Run Rust tests**

Run: `cargo test -p vibeshell`
Expected: PASS except existing ignored integration tests.

**Step 2: Run CLI tests**

Run: `cargo test -p vshell`
Expected: PASS

**Step 3: Run Rust type check**

Run: `cargo check -p vibeshell`
Expected: PASS

**Step 4: Run frontend build required by repo policy**

Run: `npm run build`
Expected: PASS

**Step 5: Commit**

```bash
git add README.md
git commit -m "docs: document headless cli workflows"
```
