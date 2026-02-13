# VibeShell — Agent Guide

## Overview
VibeShell is a modern SSH/SFTP desktop terminal built with **Tauri 2** (Rust backend) and **React 18** (TypeScript frontend). It supports multi-session SSH, SFTP file management, SSH tunneling, local shell, session recording, jump hosts, and AI tool integration.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Frontend (React 18 + Vite 6 + TypeScript)              │
│  ├── Zustand stores (10 stores)                         │
│  ├── xterm.js terminal emulator                         │
│  └── Tailwind CSS (Tokyo Night theme)                   │
├─────────────────────────────────────────────────────────┤
│  IPC: Tauri invoke (frontend↔backend)                   │
│  IPC: Named pipe / Unix socket (CLI↔backend)            │
├─────────────────────────────────────────────────────────┤
│  Backend (Rust / Tauri 2)                               │
│  ├── SSH: russh 0.44 (async, non-Clone Handle/Channel)  │
│  ├── SFTP: russh-sftp                                   │
│  ├── DB: rusqlite (SQLite, bundled)                     │
│  ├── Async: tokio runtime                               │
│  └── Local Shell: portable-pty                          │
└─────────────────────────────────────────────────────────┘
```

## Critical Knowledge

### russh 0.44 Constraints
- `client::Handle<H>` and `Channel<S>` are **NOT Clone**
- Handle sharing: wrap in `Arc<tokio::sync::Mutex<Option<Handle>>>`
- Channel I/O: use `channel.into_stream()` → `tokio::io::split()` for bidirectional data

### Frontend → Backend Communication
- All Tauri calls go through `src/lib/tauri.ts` → `safeInvoke<T>(command, args)`
- Returns `InvokeResult<T>` = `{ success: true; data: T }` | `{ success: false; error: TauriError }`
- For latency-sensitive input: use `sendInputBatched()` with RAF batching
- For fire-and-forget: use `fireAndForgetInvoke()`

### Session Lifecycle
```
SessionManager.create_with_credentials()
  → SshClient.connect_password/connect_key()
  → SshClient.open_shell()
  → Session { input: mpsc::Sender, output: broadcast::Sender }
  → Frontend attaches via Tauri event listener
```

### Theme System
- CSS variables set on `document.documentElement` from `settingsStore`
- Tailwind classes: `bg-tokyo-bg`, `text-tokyo-fg`, `border-tokyo-bg-hl`, etc.
- **NEVER** use hardcoded hex colors — always use `tokyo-*` Tailwind utilities

## Project Structure

```
src/                          # Frontend
  ├── components/             # React components (each in own folder)
  ├── stores/                 # Zustand stores (10 stores)
  ├── lib/                    # Utilities (tauri.ts, utils.ts)
  ├── types/                  # TypeScript type definitions
  ├── App.tsx                 # Main layout
  └── styles.css              # Global styles + Tailwind

src-tauri/                    # Rust backend
  ├── src/
  │   ├── commands/           # Tauri command handlers
  │   ├── ssh/                # SSH client (russh)
  │   ├── sftp/               # SFTP operations
  │   ├── session/            # Session + SessionManager
  │   ├── tunnel/             # SSH tunneling (local/remote/dynamic)
  │   ├── logging/            # Session recording
  │   ├── storage/            # Database + models
  │   ├── local_shell/        # Local terminal (portable-pty)
  │   ├── ipc/                # CLI↔GUI IPC socket
  │   ├── mcp/                # MCP server for AI tools
  │   ├── install/            # AI tool skill installer
  │   └── lib.rs              # App entry, command registration
  └── Cargo.toml

cli/                          # CLI client (workspace member)
```

## Database Schema (SQLite)

| Table | Purpose |
|-------|---------|
| `servers` | SSH server configs (host, port, auth, jump_host, post_login_cmd) |
| `groups` | Server organization groups |
| `credentials` | Encrypted credential storage |
| `server_credentials` | Per-server saved credentials |
| `tunnel_configs` | Persistent SSH tunnel configurations |
| `command_snippets` | Saved command templates |
| `recordings` | Session recording metadata |
| `settings` | App settings (key-value) |

## Stores (Frontend State)

| Store | Purpose |
|-------|---------|
| `serverStore` | Server/group CRUD |
| `sessionStore` | SSH session lifecycle |
| `localShellStore` | Local terminal sessions |
| `settingsStore` | App settings + themes |
| `fingerprintStore` | SSH host key verification |
| `notificationStore` | Toast notifications |
| `navigationStore` | View routing (main/settings) |
| `tunnelStore` | SSH tunnel configs + active tunnels |
| `snippetStore` | Command snippet management |
| `recordingStore` | Session recording state |

## Tauri Commands (70+ commands)

Organized by module: `session`, `server`, `credential`, `sftp`, `fingerprint`, `local_shell`, `snippet`, `tunnel`, `logging`, `install`, `dialog`.

All registered in `src-tauri/src/lib.rs` → `invoke_handler`.

## Build & Run

```bash
# Development
npm run dev              # Start Vite dev server
npx tauri dev            # Start Tauri dev (frontend + backend)

# Production build
npm run build            # TypeScript + Vite build
unset CI CXX CC          # Required on some systems
npx tauri build --no-bundle  # Compile release exe

# Rust only
cd src-tauri && cargo check  # Type check
cd src-tauri && cargo test   # Run tests
```

**Important:** On Windows, `unset CI CXX CC` may be needed before `tauri build` to avoid `cc-rs` toolchain detection issues.

## Testing

- **Rust tests:** `cargo test` — unit tests in modules (`storage`, `ssh`, `sftp`, `mcp`, `ipc`, `install`, `local_shell`)
- **Integration test:** `src-tauri/tests/ssh_integration_test.rs` (requires real SSH server)
- **Frontend tests:** Not yet implemented

## Code Style

- **Rust:** Standard Rust 2021 edition, `snake_case`, `log` crate for logging
- **TypeScript:** Strict mode, ES modules, React functional components with hooks
- **CSS:** Tailwind utility-first with Tokyo Night custom theme classes
- **State:** Zustand with `safeInvoke` pattern for all backend calls
- **Error handling:** `anyhow::Result` in Rust, `InvokeResult<T>` discriminated union in TS

## When Making Changes

1. **New Tauri command:** Add to `src-tauri/src/commands/`, export in `mod.rs`, register in `lib.rs` invoke_handler
2. **New frontend store:** Create in `src/stores/`, use `safeInvoke` pattern
3. **New component:** Create folder in `src/components/`, use `tokyo-*` theme classes
4. **New DB table:** Add `CREATE TABLE IF NOT EXISTS` in `database.rs::init_schema()`, add model in `models.rs`
5. **Always:** Run `npm run build` + `cargo check` before considering work complete
