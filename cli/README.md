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

Commands that need an SSH/SFTP session automatically start the native local daemon. The daemon stores its IPC endpoint and state under the current user's VibeShell data directory and can be inspected directly:

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

## Build from source

```bash
cargo build --release --package vshell --bin vibeshell
```

To prepare the target-suffixed binary consumed by Tauri desktop packaging:

```bash
node scripts/prepare-sidecar.mjs
node scripts/prepare-sidecar.mjs --target aarch64-apple-darwin
```
