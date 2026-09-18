# VibeShell — Agent Guide

## Overview
VibeShell is a modern SSH/SFTP desktop terminal built with **Tauri 2** (Rust backend) and **React 18** (TypeScript frontend). It supports multi-session SSH, SFTP file management, SSH tunneling, local shell, session recording, jump hosts, and AI tool integration.

## Branch and release workflow

All feature, fix and documentation PRs target `dev`, the default integration branch. `main` accepts release promotions from the same repository's `dev`; `master` is historical. See CONTRIBUTING.md. Version tags are explicit; ordinary pushes must not auto-bump or publish releases. VibeShell 1.1.0 is GPL-3.0-only; preserve NOTICE and third-party attribution.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Frontend (React 18 + Vite 6 + TypeScript)              │
│  ├── Zustand stores                                    │
│  ├── xterm.js terminal emulator                         │
│  └── Tailwind CSS (Tokyo Night theme)                   │
├─────────────────────────────────────────────────────────┤
│  IPC: Tauri invoke (frontend↔backend)                   │
│  IPC: Named pipe / Unix socket (CLI↔backend)            │
├─────────────────────────────────────────────────────────┤
│  Backend (Rust / Tauri 2)                               │
│  ├── SSH: russh 0.63 (async, shared session ownership)  │
│  ├── SFTP: russh-sftp                                   │
│  ├── DB: rusqlite (SQLite, bundled)                     │
│  ├── Async: tokio runtime                               │
│  └── Local Shell: portable-pty                          │
└─────────────────────────────────────────────────────────┘
```

## Critical Knowledge

### SSH ownership constraints
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

### Security Model
- Credentials are **encrypted at rest**: AES-256-GCM with a device-local key (`src-tauri/src/storage/crypto.rs`); encrypted values carry an `enc:v1:` prefix and a startup migration encrypts legacy plaintext rows
- SSH host-key verification is **TOFU, enforced server-side** in the connect flow: `probe_host_key` → user approval → connect; a changed host key hard-fails before authentication
- Terminal output events are **base64-encoded and coalesced** — the frontend must base64-decode event payloads before writing to xterm
- The CLI↔GUI IPC directory is per-user **0700**, with a private socket. GUI startup owns the service when available or attaches to an existing daemon. Route each operation to its session's owner.
- Tests must use injected temporary databases/keys. Never call production data initializers in test helpers or run all ignored tests against user configuration.
- Credentials must never enter Debug/activity output. Preserve exact secret bytes and atomic metadata/credential updates.

### Theme System
- CSS variables set on `document.documentElement` from `settingsStore`
- Tailwind classes: `bg-tokyo-bg`, `text-tokyo-fg`, `border-tokyo-bg-hl`, etc.
- **NEVER** use hardcoded hex colors — always use `tokyo-*` Tailwind utilities

## Project Structure

```
src/                          # Frontend
  ├── components/             # React components (each in own folder)
  ├── stores/                 # Zustand stores
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
  │   ├── storage/            # Database + models + credential crypto (crypto.rs)
  │   ├── local_shell/        # Local terminal (portable-pty)
  │   ├── ipc/                # CLI↔GUI IPC socket (per-user 0700)
  │   ├── mcp/                # MCP server for AI tools
  │   ├── install/            # AI tool skill installer
  │   ├── cloud_sync/         # Encrypted cloud sync vault
  │   ├── coding_agent/       # AI coding-agent gateway (command approvals)
  │   ├── dbconn/             # Database connection management
  │   ├── platform/           # Platform/window integration
  │   ├── plugins/            # Plugin runtime
  │   └── lib.rs              # App entry, command registration
  └── Cargo.toml

cli/                          # CLI client (workspace member)

plugins/                      # Built-in plugin catalog (workspace member: vibeshell-plugins)
├── src/lib.rs                # Plugin spec: manifest types, validation, command rendering
└── builtin/<plugin-id>/plugin.json   # One directory per built-in plugin
```

**Plugin spec:** `docs/plugin-spec.md` is normative. Built-in plugins live in `plugins/builtin/<id>/plugin.json` and must be registered in `BUILTIN_MANIFESTS` (plugins/src/lib.rs); a test enforces directory ↔ registration parity. Plugin installations sync/back up as the `plugin_installation` entity (see storage/sync.rs).

## Database Schema (SQLite)

| Table | Purpose |
|-------|---------|
| `servers` | SSH server configs (host, port, auth, jump_host, post_login_cmd) |
| `groups` | Server organization groups |
| `credentials` | Device-local credential storage (encrypted at rest, see Security Model) |
| `server_credentials` | Per-server saved credentials |
| `tunnel_configs` | Persistent SSH tunnel configurations |
| `command_snippets` | Saved command templates |
| `recordings` | Session recording metadata |
| `settings` | App settings (key-value) |

## Stores (Frontend State)

Stores in `src/stores/` (`cloudSyncCoordinator.ts` is a non-store orchestrator and is not listed):

| Store | Purpose |
|-------|---------|
| `agentApprovalStore` | Approvals for dangerous AI-agent commands (Agent Gateway) |
| `agentActivityStore` | Durable operation history, incremental recovery and pagination |
| `cloudSyncStore` | Cloud sync provider pairing/vault state |
| `commandHistoryStore` | Per-server command history |
| `dbConnectionsStore` | Database connection profiles |
| `fileWorkspaceStore` | Open local/SFTP files + viewer state |
| `fingerprintStore` | SSH host key verification |
| `localShellStore` | Local terminal sessions |
| `navigationStore` | View routing (main/settings) |
| `notificationStore` | Toast notifications |
| `pluginStore` | Installed plugin records (fetch/install/export) |
| `pluginWorkspaceStore` | Plugin panels opened per (session, plugin) |
| `recordingStore` | Session recording state |
| `runtimeCapabilitiesStore` | Runtime capability flags (platform, local shell, agent gateway, updater) |
| `serverStore` | Server/group CRUD |
| `sessionStore` | SSH session lifecycle |
| `settingsStore` | App settings + themes + AI tool config |
| `snippetStore` | Command snippet management |
| `tunnelStore` | SSH tunnel configs + active tunnels |
| `updateStore` | App update check/download state |

## Tauri Commands (135+ commands)

Organized by module under `src-tauri/src/commands/`: `session`, `server`, `sftp`, `fingerprint`, `local_shell`, `snippet`, `tunnel`, `logging`, `install`, `dialog`, `settings`, `cloud_sync`, `dbconn`, `plugin`, `coding_agent`, `local_files`, `workspace_window`, `history`, `agent`, `app`, `platform`.

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

- **Rust tests:** `cargo test` from the repo root (single Cargo workspace: `src-tauri`, `cli`, `plugins`) — unit tests live inside modules (`storage`, `ssh`, `sftp`, `mcp`, `ipc`, `install`, `local_shell`, …), integration tests in `src-tauri/tests/`
- **SSH integration tests:** run `bash scripts/test-ssh-compatibility.sh` for a generated-key, loopback-only Docker fixture. Other ignored tests require their own explicit external fixtures; never run them wholesale against saved user data.
- **Frontend tests:** `npm test` (Vitest, see `vitest.config.ts`) — `*.test.ts` files colocated with stores/components; CI runs them
- **Formatting:** `cargo fmt --check` gates CI — run `cargo fmt` before committing Rust changes

Pre-completion gates remain `npm run build` + `cargo check` (see When Making Changes).

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
