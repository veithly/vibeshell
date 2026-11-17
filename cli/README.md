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
vibeshell servers add user@host:2222 --name my-server --identity ~/.ssh/id_ed25519
vibeshell servers delete my-server --yes
vibeshell ssh <server>
vibeshell sftp <server>
vibeshell sftp --session ALIAS pwd
```

Commands that need an SSH/SFTP session automatically start the native local daemon. The daemon uses a private per-user IPC endpoint and the same saved-profile database as the desktop. A GUI-owned service can also answer native CLI requests. Inspect the current owner and sessions with:

```bash
vibeshell daemon status
vibeshell sessions
```

`servers add` also accepts `--user`, `--port`, `--jump`, `--group`, repeatable `--tag`, `--agent-forwarding`, and `--post-login`. Passwords come from an already-set `SSH_PASSWORD` or `VIBESHELL_PASSWORD`; encrypted keys use `VIBESHELL_KEY_PASSPHRASE` or `SSH_KEY_PASSPHRASE`. Do not place actual secrets in command arguments or shell history. Secret whitespace is preserved. The profile, optional group, encrypted credentials and sync metadata commit together. Deletion requires `--yes`, closes that server's sessions, and removes its saved credentials atomically with its profile. Ambiguous names/IDs and malformed targets fail without deletion.

The supported executable is `vibeshell`, not the retired `vshell` command. `vshell` remains only the internal Cargo package identifier used by existing build scripts.

## Container sessions and network settings

Use `vibeshell containers list --session ID` and `vibeshell containers open --session ID CONTAINER` to open a persistent child of a connected SSH session. The returned session ID works with `attach`, `ssh-session` and plugins. Container identity is preserved across GUI/CLI/MCP ownership; closing it does not stop the container. Container SFTP is explicitly unavailable rather than exposing host files.

Use `vibeshell proxy show` or `vibeshell proxy set --mode custom --url socks5h://127.0.0.1:1080` for the device-local outbound route. Authenticated proxies accept `--username` and protected `--password-stdin`, never a password flag or URL credential. New connections use the saved policy without direct fallback. [Detailed setup, protocol matrix and boundaries](../docs/NETWORK_AND_CONTAINERS.md).

## Import sources

Teleport nodes can be added with `vibeshell servers add alice@node --type teleport --proxy proxy.example:443` or previewed with `vibeshell import teleport --proxy proxy.example:443 --user alice --dry-run`. Run `tsh login` explicitly first. The Unix login is required; automatic import never contacts Teleport. Terminal and command operations share the current service, while file operations require a standard external `sftp-server` executable on the node. See [Teleport setup and transport boundaries](../docs/TELEPORT.md).

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

The desktop displays Agent (CLI) commands in the same history as human terminal input. `ssh`, `exec` and `ssh-session` use the same persistent remote shell, so `cd` and `export` survive subsequent CLI calls. For shells emitting OSC 3008 command status events, the CLI propagates the remote exit code, not merely command submission; if no exit status is available it prints a warning instead of claiming verified success. Connections require their owning process to remain alive; upgrade the GUI and CLI together rather than mixing incompatible service versions.

VibeShell is GPL-3.0-only. Release archives include LICENSE, NOTICE and legacy attribution; matching source materials are available beside the release binaries. Earlier MIT grants are not revoked. The software comes without warranty to the extent permitted by law.

## Build from source

```bash
cargo build --release --locked --package vshell --bin vibeshell
```

To prepare the target-suffixed binary consumed by Tauri desktop packaging:

```bash
node scripts/prepare-sidecar.mjs
node scripts/prepare-sidecar.mjs --target aarch64-apple-darwin
```
