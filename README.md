<div align="center">

<img src="app-icon.svg" width="128" height="128" alt="VibeShell Logo" />

# VibeShell

**A modern, high-performance SSH/SFTP terminal client built with Tauri + React**

[![Release](https://img.shields.io/github/v/release/veithly/vibeshell?style=flat-square&color=7aa2f7)](https://github.com/veithly/vibeshell/releases)
[![License](https://img.shields.io/badge/license-MIT-9ece6a?style=flat-square)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/veithly/vibeshell/ci.yml?style=flat-square&label=CI)](https://github.com/veithly/vibeshell/actions)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-bb9af7?style=flat-square)](#installation)

<br />

<sub>Built with [Tauri 2](https://v2.tauri.app) · [React 18](https://react.dev) · [Xterm.js](https://xtermjs.org) · [russh](https://github.com/warp-tech/russh)</sub>

---

[Features](#features) · [Installation](#installation) · [Screenshots](#screenshots) · [Development](#development) · [Contributing](#contributing)

</div>

<br />

## Features

### Core Terminal
- **Multi-tab sessions** — Connect to multiple servers simultaneously with tabbed interface
- **High-performance rendering** — WebGL-accelerated terminal via Xterm.js
- **Local shell** — Built-in local terminal alongside SSH sessions
- **Keyboard shortcuts** — `Ctrl+N` new server, `Ctrl+K` quick command, `Ctrl+W` close tab

### SSH & Security
- **SSH tunneling** — Local forward, remote forward, and SOCKS5 dynamic forwarding
- **Jump host (ProxyJump)** — Connect through bastion/jump servers
- **SSH agent forwarding** — Forward local SSH agent to remote hosts
- **Host key verification** — TOFU with SHA-256 fingerprint management
- **Credential storage** — Encrypted local storage for passwords and key passphrases

### File Management
- **SFTP browser** — Built-in file manager with upload, download, preview
- **Drag & drop** — Upload files by dragging into the SFTP panel
- **Compression** — Compress/extract archives on remote servers

### Productivity
- **Command snippets** — Save and reuse frequently used commands
- **Session recording** — Record terminal sessions for playback and audit
- **Post-login commands** — Auto-execute commands after SSH connection
- **Quick command palette** — `Ctrl+K` to search and run commands
- **Server groups** — Organize servers into logical groups

### Customization
- **Tokyo Night theme** — Beautiful dark theme with customizable color scheme
- **Internationalization** — English and Chinese (简体中文) language support
- **Configurable terminal** — Font size, cursor style, scrollback buffer, and more

### Integration
- **MCP support** — Model Context Protocol for AI tool integration
- **CLI companion** — `vibeshell` CLI for scripted operations
- **Cross-platform** — Native performance on Windows, macOS, and Linux

<br />

## Installation

### Download

| Platform | Download |
|----------|----------|
| **Windows** x64 | [`.exe` installer](https://github.com/veithly/vibeshell/releases/latest) · [`.msi`](https://github.com/veithly/vibeshell/releases/latest) |
| **macOS** Apple Silicon | [`.dmg`](https://github.com/veithly/vibeshell/releases/latest) |
| **macOS** Intel | [`.dmg`](https://github.com/veithly/vibeshell/releases/latest) |
| **Linux** x64 | [`.deb`](https://github.com/veithly/vibeshell/releases/latest) · [`.AppImage`](https://github.com/veithly/vibeshell/releases/latest) |

> Download the latest release from the [Releases](https://github.com/veithly/vibeshell/releases) page.

### Build from Source

**Prerequisites:**
- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) 1.70+
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform

```bash
# Clone the repository
git clone https://github.com/veithly/vibeshell.git
cd vibeshell

# Install frontend dependencies
npm install

# Development mode (hot reload)
npx tauri dev

# Build release binary
npx tauri build
```

<br />

## Screenshots

<div align="center">

> Screenshots coming soon. Run `npx tauri dev` to see VibeShell in action.

</div>

<br />

## Architecture

```
┌─────────────────────────────────────────────┐
│                  Frontend                    │
│  React 18 + Zustand + Xterm.js + Tailwind   │
├─────────────────────────────────────────────┤
│              Tauri IPC Bridge               │
├─────────────────────────────────────────────┤
│                  Backend                     │
│  Rust + russh + rusqlite + tokio             │
│  ┌──────┐ ┌──────┐ ┌───────┐ ┌──────────┐  │
│  │ SSH  │ │ SFTP │ │Tunnel │ │ Storage  │  │
│  └──────┘ └──────┘ └───────┘ └──────────┘  │
└─────────────────────────────────────────────┘
```

- **Frontend:** React handles UI, Zustand manages state, Xterm.js renders terminals
- **Backend:** Rust provides native SSH/SFTP via russh, SQLite for persistence
- **Communication:** Tauri IPC with `safeInvoke()` pattern for type-safe calls

<br />

## Development

```bash
# Start dev server with hot reload
npx tauri dev

# Type-check frontend
npx tsc --noEmit

# Check Rust backend
cargo check --manifest-path src-tauri/Cargo.toml

# Run Rust tests
cargo test --manifest-path src-tauri/Cargo.toml

# Build release (Windows)
set CI= && set CXX= && set CC= && npx tauri build
```

### Project Structure

```
vibeshell/
├── src/                    # React frontend
│   ├── components/         # UI components
│   ├── stores/             # Zustand state stores
│   ├── i18n/               # Internationalization
│   ├── lib/                # Utilities (tauri.ts, utils.ts)
│   └── types/              # TypeScript type definitions
├── src-tauri/              # Rust backend
│   └── src/
│       ├── commands/       # Tauri command handlers
│       ├── ssh/            # SSH client (russh)
│       ├── session/        # Session management
│       ├── storage/        # SQLite database
│       ├── tunnel/         # SSH tunneling engine
│       └── logging/        # Session recording
├── cli/                    # CLI companion tool
├── .github/workflows/      # CI/CD pipelines
└── docs/                   # Documentation
```

<br />

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting a PR.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

<br />

## License

[MIT](LICENSE) — Made with care by [veithly](https://github.com/veithly).
