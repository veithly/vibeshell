# Native VibeShell CLI

The `vibeshell` binary is the headless SSH/SFTP client and local daemon shipped with VibeShell. It shares the same data model as the desktop application but does not require a window system, Node.js, Electron, or a running desktop process.

## Install a release archive

Linux and macOS:

```bash
./install.sh
```

The default destination is `~/.local/bin/vibeshell`. Override it with `VIBESHELL_INSTALL_DIR=/usr/local/bin ./install.sh` when appropriate.

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

Both installers verify the native binary and trigger its built-in, idempotent Skill installer. The canonical VibeShell Skill is then written to every detected coding-agent directory and to `~/.agents/skills/vibeshell` without installing Node.js or a separate npm package.

## Use on a headless server

```bash
vibeshell version
vibeshell import auto --dry-run
vibeshell import auto
vibeshell servers
vibeshell ssh <server>
vibeshell sftp <server>
```

Commands that need an SSH/SFTP session automatically start the native local daemon. The daemon uses a private per-user IPC endpoint and the same saved-profile database as the desktop. A GUI-owned service can also answer native CLI requests. Inspect the current owner and sessions with:

```bash
vibeshell daemon status
vibeshell sessions
```

## Import sources

```bash
vibeshell import openssh --path ~/.ssh/config
vibeshell import tabby --path ~/.config/tabby/config.yaml
vibeshell import putty --path ~/putty-sessions.reg
```

OpenSSH `Host`, `Include`, `IdentityFile`, `ProxyJump`, `RemoteCommand`, and `ForwardAgent` metadata are supported. Tabby SSH profiles and PuTTY sessions/registry exports are supported. Third-party stored passwords are deliberately not copied. OpenSSH-format private keys are referenced by local path and read only when a connection is established; PuTTY `.ppk` keys must first be converted to OpenSSH format.

## Agent-readable plugins and license

```bash
vibeshell plugins list --installed --json
vibeshell plugins describe server-performance
vibeshell plugins docs server-performance
vibeshell plugins run server-performance status --session SESSION_ID --inputs '{}'
vibeshell license
```

Select an existing session and verify the plugin is installed and enabled first. Plugin action details are in the packaged `skills/vibeshell/references/` documents or available live through `plugins docs`; the main Skill contains discovery instructions and an index. CLI and MCP share permission and input checks. Confirmation and sudo opt-in require the person's authorization.

The desktop displays Agent/CLI operations in activity history. Use shared terminal commands when collaborating with a person and independent exec for non-interfering inspection. Inputs marked as sent are not proof of remote command success. Connections require their owning process to remain alive; upgrade the GUI and CLI together rather than mixing incompatible service versions.

VibeShell 1.1.0 is GPL-3.0-only. Release archives include LICENSE, NOTICE and legacy attribution; matching source materials are available beside the release binaries. Earlier MIT grants are not revoked. The software comes without warranty to the extent permitted by law.

## Build from source

```bash
cargo build --release --locked --package vshell --bin vibeshell
```

To prepare the target-suffixed binary consumed by Tauri desktop packaging:

```bash
node scripts/prepare-sidecar.mjs
node scripts/prepare-sidecar.mjs --target aarch64-apple-darwin
```
