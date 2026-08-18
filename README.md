<div align="center">

<img src="app-icon.svg" width="112" height="112" alt="VibeShell logo" />

# VibeShell

**An AI-native SSH, SFTP, tunnel, and local terminal workspace for developers who ship from the command line.**

[![Release](https://img.shields.io/github/v/release/veithly/vibeshell?style=flat-square&color=7aa2f7)](https://github.com/veithly/vibeshell/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/veithly/vibeshell/ci.yml?style=flat-square&label=CI&color=9ece6a)](https://github.com/veithly/vibeshell/actions/workflows/ci.yml)
[![Tauri 2](https://img.shields.io/badge/Tauri-2.x-24c8db?style=flat-square)](https://v2.tauri.app)
[![React](https://img.shields.io/badge/React-18-61dafb?style=flat-square)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-native-f7768e?style=flat-square)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/Windows%20%7C%20macOS%20%7C%20Linux-supported-bb9af7?style=flat-square)](#install)

[Screenshots](#screenshots) · [Recent Updates](#recent-updates) · [Why It Exists](#why-it-exists) · [Features](#features) · [AI Agents](#ai-agents) · [Install](#install) · [Build](#build-from-source)

</div>

![VibeShell terminal workspace](docs/assets/screenshots/terminal-workspace.png)

VibeShell is a native desktop terminal and headless SSH/SFTP service for people and AI agents working on the same machines. It combines a polished Tauri workspace with the standalone Rust `vibeshell` command, a reusable local daemon, SSH configuration import, tunnels, session recording, and optional MCP integration.

If you have ever asked an AI coding agent to deploy, inspect logs, edit a remote config, or move files over SSH, VibeShell gives it a real, reusable, observable workspace instead of a pile of brittle one-off shell commands. The desktop application may be closed: the same saved servers and sessions remain available through the native CLI on a workstation or a server without a graphical desktop.

> The screenshots in this README are rendered with Playwright from VibeShell's real React components using sanitized demo servers. They use reserved example domains only; no real hostnames, IPs, credentials, or customer data are shown.

## Screenshots

| Server launcher | Split terminal workspace |
| --- | --- |
| ![Icon view with three sanitized demo servers](docs/assets/screenshots/server-launcher.png) | ![Three shell panes in the VibeShell workspace](docs/assets/screenshots/terminal-workspace.png) |

| Finder-style SFTP | Theme system |
| --- | --- |
| ![Three-column SFTP file manager with sanitized demo paths](docs/assets/screenshots/sftp-workflow.png) | ![Five high-contrast themes in English settings](docs/assets/screenshots/theme-system.png) |

## Recent Updates

- **Native `vibeshell` command and headless daemon**: use saved servers, persistent SSH sessions, remote commands, file editing, and SFTP without opening the desktop application or installing Node.js.
- **SSH profile import**: preview or import OpenSSH config files, PuTTY sessions/registry exports, and Tabby SSH profiles, including key paths, ports, ProxyJump relationships, startup commands, and agent forwarding where supported.
- **Bundled Coding Agent Skill**: the canonical Skill ships with the Rust binary and is installed idempotently into detected agent directories plus `~/.agents/skills`; generated Skills call `vibeshell` directly and never require `gateway.mjs`.
- **Coding agents inside the terminal workspace**: launch Claude Code, Codex, OpenCode, or Pi in a native terminal tab, choose a repository and supported start/access mode, then inspect its live Git status and unified diffs without leaving VibeShell.
- **Remote file workspace**: open SFTP files in shared tabs, edit syntax-highlighted text with save protection, preview images, PDF, audio, and video, and search the contents of ZIP, TAR, TAR.GZ, and TGZ archives.
- **Encrypted cloud sync**: pair devices through a private GitHub Gist or WebDAV file and sync servers, groups, and command snippets as AES-256-GCM ciphertext. Credentials, host fingerprints, recordings, and live sessions remain device-local; vault keys are session-only until secure Keychain/Keystore persistence lands.
- **Stronger SFTP workflows**: multi-select batch actions, visible upload/download progress, bounded large-file previews, archive compress/extract actions, and safer delete and transfer handling.
- **Extensible session tools**: install external command plugins from the marketplace with explicit local/remote execution permissions and guarded `sudo` support.
- **Mobile-ready runtime**: capability-gated iOS/Android foundations, foreground SSH/SFTP, touch terminal keys, safe-area layouts, and responsive workspace actions. Native mobile file pickers, persistent credentials, and background SSH are intentionally deferred.

### New In 1.0

- **Split shells, one workspace**: open up to four terminal panes and switch between row and column layouts without leaving the active session.
- **Finder-style SFTP browsing**: traverse multiple path levels in a column view, switch to an icon view, preview files in place, and expand or collapse the complete SFTP workspace.
- **A quieter native shell**: VibeShell opens directly into a local terminal, keeps SSH, SFTP, split, and settings actions in a compact icon toolbar, and reveals labels through hover tooltips.
- **Five contrast-safe themes**: Paper White, Warm Ivory, Ink Black, Violet Black, and Cyan Black keep titles, controls, terminal text, and hover states readable on both light and dark surfaces.
- **Platform-aware window motion**: macOS uses native traffic-light and fullscreen behavior, while Windows and Linux receive controls shaped for their platform conventions.
- **Smarter command interaction**: Zsh starts as an interactive login shell, ghost-text completion stays close to the cursor, and a mouse click can reposition the terminal input cursor.

## Why It Exists

Traditional SSH clients assume a single human is typing everything. AI coding agents changed that: now the operator and assistant need to share enough visible context to act safely.

VibeShell is designed around that new workflow:

- **Human-friendly by default**: fast tabs, xterm.js rendering, command snippets, local shell, SFTP, tunnels, and session recording.
- **Agent-ready when needed**: the bundled native Skill exposes server, session, command, search, file, and SFTP workflows to compatible coding agents; the authenticated MCP layer remains available for GUI-shared workflows.
- **Observable automation**: agents can reuse persistent sessions from the native CLI on a headless host, or operate alongside you in the visible desktop control room.
- **Native and local-first**: Rust backend, SQLite storage, optional device-local credentials, and no hosted control plane.

## Features

### Terminal Workspace

- Multi-tab SSH sessions and local shell sessions.
- Native terminal tabs for Claude Code, Codex, OpenCode, and Pi coding agents.
- Live workspace-change panel with staged/unstaged state, rename and conflict detection, and bounded unified diffs.
- Up to four shell panes with horizontal and vertical split layouts.
- Direct-to-terminal startup with no empty landing screen.
- Smooth terminal rendering with xterm.js and WebGL support.
- Mouse-based input cursor placement for faster command editing.
- Interactive login-shell startup and ghost-text completions for common terminal commands.
- Compact icon toolbar with hover tooltips and responsive overflow behavior.
- Command snippets with search, tags, copy, and insert actions.
- Terminal session recording for audit, replay, and handoff.

### Native UI And Themes

- Platform-specific window controls for macOS, Windows, and Linux.
- Native macOS fullscreen transitions that preserve close and minimize behavior.
- Paper White, Warm Ivory, Ink Black, Violet Black, and Cyan Black themes.
- WCAG AA-tested text contrast across the five built-in themes.
- Responsive actions that keep common icons visible and fold secondary actions into a compact menu.

### SSH, SFTP, And Tunnels

- Password, key, and key-with-passphrase authentication.
- Optional device-local credentials and per-server configuration. Secure Keychain/Keystore storage is planned; current saved credentials are not encrypted at rest.
- Host key verification with trusted fingerprint management.
- Jump host / ProxyJump support for bastion access.
- Import from OpenSSH, PuTTY, and Tabby with a safe dry-run/JSON preview.
- Imported OpenSSH private keys remain referenced by local path and are read only when establishing a connection; third-party stored passwords are not copied.
- SSH agent forwarding.
- Finder-style SFTP column browsing plus an icon view for scanning folders visually.
- Expandable SFTP workspace with a dedicated address row and responsive action menu.
- Shared file-workspace tabs with syntax-aware text editing, rich media previews, and searchable archive listings.
- File preview, multi-select upload/download/delete, rename, mkdir, compression, extraction, recursive upload, and sync flows with visible progress.
- Local forward, remote forward, and dynamic SOCKS5 tunnels.

### Cloud Sync And Mobile

- End-to-end encrypted server, group, and command-snippet sync through GitHub Gist or WebDAV.
- Deterministic revisions, tombstones, conflict reporting, retryable outbox delivery, and portable JSON import/export.
- Credentials, trusted fingerprints, recordings, tunnels, and runtime sessions stay out of cloud sync.
- Foreground SSH terminal and remote SFTP browsing on the mobile runtime foundation, with desktop-only features hidden through backend capability detection.

### Local Coding Agents

VibeShell can detect installed Claude Code, Codex, OpenCode, and Pi CLIs and launch them directly in the visible terminal workspace. Each tool exposes only the session and access modes it supports. Agent sessions keep their repository path attached, so the workspace-change panel can refresh Git status and render text diffs while the agent works.

### AI Agents

VibeShell ships one canonical Skill together with the native `vibeshell` executable. The first desktop or CLI launch installs that Skill idempotently into every detected coding-agent directory and into the shared `~/.agents/skills/vibeshell` convention. Claude Code, Codex, Cursor, OpenCode, Gemini CLI, Windsurf, Roo Code, Continue, Kiro, Trae, OpenHands, and other supported tools can therefore discover VibeShell without a separate Skill package or npm installation.

The installed Skill calls the Rust CLI directly. It does not run Node.js, `gateway.mjs`, browser automation, or raw `ssh` commands that bypass VibeShell's saved server and session model. Commands that need a connection automatically start the native local daemon, so the same workflow works with the desktop closed or on a headless server.

| Area | Native commands |
| --- | --- |
| Inventory and import | `vibeshell servers`, `vibeshell import auto --dry-run`, `vibeshell import auto` |
| Sessions | `vibeshell ssh`, `vibeshell sessions`, `vibeshell attach`, `vibeshell ssh-session`, `vibeshell kill` |
| Remote commands and search | `vibeshell ssh <server> -- <command>`, `vibeshell rg` |
| Remote files | `vibeshell get-content`, `vibeshell add-file`, `vibeshell edit-file` |
| SFTP | `vibeshell sftp <server> ls|get|put|sync|...` or the interactive SFTP prompt |
| Diagnostics | `vibeshell daemon status`, `vibeshell version` |

The authenticated MCP Gateway remains available for workflows that intentionally share the visible GUI session manager, but it is no longer a prerequisite for the installed Skill.

Example intent:

```text
You: "Check the demo API logs, patch the config, upload the build, and restart the service."

Agent:
1. Finds the saved server with `vibeshell servers`.
2. Opens or reuses a persistent native session.
3. Reads logs with `ssh`/`rg` and inspects the current remote file.
4. Applies the guarded edit and uploads the build through SFTP.
5. Restarts the service, reads the exit status and output, and reports the verified result.
```

## Architecture

```text
VibeShell
├─ React 18 + TypeScript + Zustand + Tailwind
│  ├─ Server list, session tabs, terminal, SFTP, tunnels, settings
│  ├─ Coding-agent launcher, file workspace, and live Git diff panel
│  └─ safeInvoke wrappers for all Tauri IPC calls
├─ Tauri 2 desktop bridge
├─ Native `vibeshell` CLI
│  ├─ auto-starting headless IPC daemon
│  ├─ SSH/session/command/file/SFTP commands
│  └─ OpenSSH, PuTTY, and Tabby import
├─ Shared Rust core
│  ├─ russh SSH client and persistent session manager
│  ├─ russh-sftp operations and recursive sync helpers
│  ├─ SQLite storage and device-local credentials/key references
│  ├─ SSH tunnel and local shell managers
│  ├─ encrypted cloud sync and portable workspace snapshots
│  ├─ local coding-agent launcher and bounded Git inspection
│  └─ optional authenticated MCP Gateway sharing the session manager
└─ Bundled canonical Skill + multi-agent installer
```

## Install

Download the latest installer from [GitHub Releases](https://github.com/veithly/vibeshell/releases).

| Platform | Desktop app | Headless/native CLI |
| --- | --- | --- |
| Windows x64 | `.exe` / `.msi` | `VibeShell-CLI-*-windows-x64.zip` |
| macOS Apple Silicon | `.dmg` | `VibeShell-CLI-*-macos-arm64.tar.gz` |
| macOS Intel | `.dmg` | `VibeShell-CLI-*-macos-x64.tar.gz` |
| Linux x64 | `.deb` / `.AppImage` / `.rpm` | `VibeShell-CLI-*-linux-x64.tar.gz` |

Desktop installers include the native CLI and canonical Skill. Windows adds the application directory to the user `PATH`; macOS and Linux persist the bundled command at `~/.local/bin/vibeshell` on application startup. The standalone headless archives contain the same binary, Skill, and `install.sh` or `install.ps1` for servers where no desktop is available. No separate Node.js/npm runtime is required.

After installation:

```bash
vibeshell version
vibeshell import auto --dry-run
vibeshell import auto
vibeshell servers
vibeshell ssh <server>
```

## Build From Source

Prerequisites:

- Node.js 18 or newer.
- Rust stable.
- Tauri platform prerequisites for your OS.
- On Windows, Visual Studio Build Tools with the C++ workload.

```bash
git clone https://github.com/veithly/vibeshell.git
cd vibeshell
npm ci
npm run build
cargo check --workspace
cargo test --workspace
```

Run in development:

```bash
npm run dev
npx tauri dev
```

Build release binaries:

```bash
# Frontend + Rust checks
npm run build
cargo check --workspace
cargo test --workspace

# Desktop app with the target-suffixed native CLI sidecar.
npm run build:desktop

# Standalone headless CLI.
cargo build --release --package vshell --bin vibeshell
```

Windows installer packaging:

```powershell
powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/build-msi.ps1 -NoPause
```

The Windows packaging script builds the native `vibeshell.exe` sidecar first, then produces NSIS and MSI installers that expose the command through the user `PATH` and carry the bundled Skill.

## CI And Release

GitHub Actions run:

- Frontend type-check and Vite build.
- Frontend unit/component tests.
- Rust `cargo check` and `cargo test` on Linux, Windows, and macOS.
- Rust target check for the iOS arm64 simulator.
- Linux `cargo clippy -- -D warnings`.
- Release builds for Windows x64, macOS arm64/x64, and Linux x64.
- Target-specific native CLI sidecars inside desktop installers.
- Standalone headless CLI archives containing the binary, installer, README, and canonical Skill.

The release workflow can be triggered from tags (`v*`) or manually with a patch/minor/major bump.

## Project Layout

```text
src/                    React frontend
src/components/         Terminal, SFTP, settings, tunnels, dialogs
src/stores/             Zustand stores for app state
src/i18n/               English and Simplified Chinese locales
src/lib/tauri.ts        safeInvoke and IPC helpers
src-tauri/              Rust/Tauri backend
src-tauri/src/ssh/      SSH client and host fingerprints
src-tauri/src/sftp/     SFTP operations and sync helpers
src-tauri/src/ssh_import/ OpenSSH, PuTTY, and Tabby profile parsers
src-tauri/src/mcp/      Optional MCP tools and transports
src-tauri/src/install/  Native CLI and multi-agent Skill installer
src-tauri/src/cloud_sync/ Encrypted GitHub Gist and WebDAV sync
src-tauri/src/coding_agent/ Local coding-agent launch and Git workspace inspection
cli/                    primary native CLI and headless daemon
skills/vibeshell/        canonical Coding Agent Skill
.claude/skills/          Claude-compatible Skill mirror
.codex/skills/           Codex-compatible Skill mirror
scripts/                 sidecar and headless installer/release tooling
docs/                   Design notes, plans, and README screenshots
```

## Contributing

Issues and pull requests are welcome. The best contributions are small, verified, and focused:

1. Search existing issues and code paths first.
2. Keep behavior changes narrow.
3. Add or update focused tests when behavior changes.
4. Run `npm run build`, `cargo check`, and `cargo test` before opening a PR.

If VibeShell helps your remote workflow, a star makes the project easier for other agent-powered developers to discover.
