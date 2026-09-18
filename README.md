<div align="center">
  <img src="app-icon.svg" width="96" alt="VibeShell" />
  <h1>VibeShell</h1>
  <p><strong>Your terminal. Your agent. The same workspace.</strong></p>
  <p>A local-first SSH/SFTP workspace for people and coding agents — with visible operations, shared sessions, integrated files, and discoverable plugins.</p>

  [English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

  [![CI](https://github.com/veithly/vibeshell/actions/workflows/ci.yml/badge.svg?branch=dev)](https://github.com/veithly/vibeshell/actions/workflows/ci.yml)
  [![Release](https://img.shields.io/github/v/release/veithly/vibeshell)](https://github.com/veithly/vibeshell/releases)
  [![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

  [Download](https://github.com/veithly/vibeshell/releases) · [What's new in 1.1](CHANGELOG.md) · [Agent guide](skills/vibeshell/SKILL.md) · [Contribute to dev](CONTRIBUTING.md)
</div>

![VibeShell terminal workspace](docs/assets/screenshots/terminal-workspace.png)

## Why a workspace, not another SSH command?

SSH already provides secure transport, authentication, forwarding, and remote command execution. VibeShell does not replace that protocol or claim a faster network. It brings the work around SSH into one place: **the session you are using, the files you are editing, and the actions your agent is taking**.

| In a command-line-only workflow | In VibeShell |
| --- | --- |
| A person and an agent may work through unrelated terminals. | Desktop, native CLI and MCP share saved targets and discoverable sessions. |
| You need to ask what the agent just ran. | Commands and operation states appear in an activity strip and durable history. |
| A new agent connection is invisible to the desktop. | New sessions become separate tabs without stealing your active tab. |
| Files, tunnels and commands require switching tools. | Local/remote file tabs, SFTP, forwarding and plugin views live alongside terminals. |
| Each automation needs handwritten command knowledge. | Plugins expose action schemas and current usage references to agents. |
| A sync tool can mistake equal file sizes for equal content. | Directory sync compares content and protects excluded paths during deletion. |

You can assemble similar workflows from OpenSSH, tmux, editors and scripts. VibeShell makes their coordination a product feature, rather than a setup task.

## The 1.1 experience

### Work with an agent without losing visibility

Agent and CLI operations show the command, target session, time and lifecycle state. Repeated commands remain separate records; long, multiline commands are retained, with pagination and recovery after reopening the UI. The activity history is encrypted locally and is not part of cloud sync.

Use the shared interactive terminal when you need to collaborate in the same shell. Use independent exec for inspection without typing into the person's prompt; it remains visible in activity history. A successful input operation means **bytes were delivered**, not that the remote command completed successfully.

When an agent opens another session, its tab appears without changing the person's selection. Session identity, not server name, distinguishes parallel work. Closing an owner process ends its connections: daemon-owned sessions can outlive the GUI, while GUI-owned sessions cannot survive quitting that GUI process.

### Less window management, more context

A searchable connection launcher brings SSH servers, local shells and coding-agent entry points together. Switch between list and card views; use keyboard focus management, light/dark themes and reduced-motion support. Card animations use browser-native APIs rather than a separate animation runtime.

Split terminals and file views, detach work into another window, and keep unsaved file edits when reorganizing the workspace. Command history, snippets, contextual actions and explicit error messages reduce repetitive copying and ambiguous “success” notifications.

![Connection launcher](docs/assets/screenshots/server-launcher.png)

### Files are part of the session

Browse SFTP in columns or icon views, select multiple files, copy paths, and follow transfer progress. Open local and remote files in workspace tabs; supported viewers include text/code, images, PDF, media and archives.

The transport is designed around integrity: transfers use bounded chunks; a download waits for local writes before reporting completion; sync detects same-size edits; excluded paths and nested `.gitignore` rules are protected when deleting extras. Local directory transfers reject overlapping source and destination roots. Content comparison can add remote reads — this is an integrity choice, not a bandwidth-saving claim.

![SFTP workflow](docs/assets/screenshots/sftp-workflow.png)

### Edit saved credentials safely

Change a saved password, replace a private key, or update/clear a key passphrase without recreating the server. Untouched fields preserve their previous values; existing secrets are not fetched into the edit form. Metadata, renames and credential changes commit together or roll back together.

This edits **VibeShell's saved login information**, not the remote operating-system account's password. Unknown or changed host keys still require the appropriate trust decision; a correct login key does not replace host identity verification.

### Native automation, not a second server inventory

The Rust `vibeshell` executable can operate without Node.js or a desktop window. It starts a native daemon when required and reuses saved profiles and sessions. When a daemon already owns SSH sessions, the GUI attaches without replacing its live socket; terminal, SFTP, tunnel, recording and database-probe operations route to the owning process.

Local coding-agent launchers support tools such as Claude Code, Codex, OpenCode and Pi through real PTYs, alongside repository status/diff views. Those agents are separately installed products: VibeShell does not include model subscriptions or their credentials.

### Plugins an agent can actually discover

Every supported built-in or imported declarative plugin exposes installation state, permissions, action inputs and a current reference. **The main Skill is an index, not an encyclopedia**; detailed instructions live in per-plugin reference documents.

```bash
vibeshell plugins list --installed --json
vibeshell plugins describe server-performance
vibeshell plugins docs server-performance
vibeshell plugins run server-performance status --session SESSION_ID --inputs '{}'
```

Replace `SESSION_ID` with an existing session, and check the plugin is installed and enabled first. `describe` returns machine-readable schemas; `docs` reflects the current validated manifest, including imported plugins.

| Built-in area | Plugins |
| --- | --- |
| Host operations | Server Performance, Process Explorer, System Logs, Network Inspector, Disk Usage |
| Services and infrastructure | Docker Containers, Kubernetes Pods, Cron Scheduler, Systemd Services |
| Data and development | Database Inspector, Redis Inspector, Git Workspace |

CLI and MCP execution enforce the same enabled-state, permission and input checks. Explicit confirmation and sudo opt-in are not implied by a documentation example. MCP uses the human approval gateway, not a model-provided approval flag. Plugins need the corresponding tools and permissions on the target; a manifest is not an installed Docker or Kubernetes environment.

[Plugin specification](docs/plugin-spec.md) · [Collaboration and API details](docs/AGENT_COLLABORATION.md) · [Plugin reference index](skills/vibeshell/SKILL.md#plugin-discovery-and-references)

## Start with your existing servers

Download the desktop installer or native CLI archive matching your platform from [Releases](https://github.com/veithly/vibeshell/releases).

| Platform | Desktop | Standalone CLI |
| --- | --- | --- |
| macOS Apple Silicon / Intel | Architecture-specific `.dmg` | Architecture-specific `.tar.gz` |
| Windows x64 | `.exe` / `.msi` | `.zip` |
| Linux x64 | `.AppImage` / `.deb` | `.tar.gz` |

Use published assets, not an unfinished draft. Apple Developer ID signing/notarization availability is stated in the release notes; a local ad-hoc signature is not Apple notarization. Mobile targets remain experimental and do not have desktop feature parity.

For a native CLI archive, inspect and run its included `install.sh` or `install.ps1`. See [CLI installation and commands](cli/README.md). Desktop packages include the CLI sidecar; Skill installation writes the bundled guide and plugin references to supported agent directories.

```bash
vibeshell version
vibeshell import auto --dry-run
# Review the preview before importing.
vibeshell import auto
vibeshell servers
vibeshell ssh my-server
```

Replace `my-server` with a saved name. OpenSSH, PuTTY and Tabby profile imports are supported; third-party stored passwords are deliberately not copied. PuTTY `.ppk` keys need conversion to OpenSSH format. You can also add and edit servers in the GUI. CLI server-create/delete and Teleport support are not part of 1.1.0.

For a command on a saved server or an existing session:

```bash
vibeshell ssh my-server -- uname -a
vibeshell sessions
# Use an alias actually returned above, not necessarily 001.
vibeshell ssh-session 001 -- pwd
vibeshell sftp my-server ls /srv/app
vibeshell sftp my-server get /srv/app/config.toml ./config.toml
```

For complex quoting, use `--command-file ./remote-command.sh` or `--command-stdin`. Request `--new` only when you need a separate connection. Never put passwords or private-key contents into command-line arguments or agent prompts.

## Forwarding, recording and optional sync

Local forwarding, SOCKS5 and reverse forwarding include listener-readiness checks, cancellation and half-close handling. Session cleanup also tears down associated tunnels and recordings. Inspect bind addresses before exposing a service beyond loopback.

Optional encrypted sync uses your configured Gist or WebDAV provider for server metadata, groups, snippets and plugin installations. It is not a VibeShell-hosted SSH relay. Login credentials, host-key trust, live terminal sessions and agent activity history stay outside that sync. Protect your provider token and recovery material; do not assume all plugin settings are harmless.

## Security and compatibility boundaries

VibeShell checks SSH host keys before authentication, including the actual destination behind a jump host. Device keys and saved credentials use local encrypted storage; on Unix, private material is permission-restricted. This is **not** a claim of OS Keychain storage or protection from a compromised user account.

Common OpenSSH password/key and PAM keyboard-interactive paths are exercised by an isolated test matrix, along with PTY, SFTP and forwarding. This does not establish support for every MFA flow, hardware token, network appliance or SSH implementation. Remote performance collection currently assumes Linux `/proc`.

Agents still need supervision. Dangerous actions may require approval; textual command classification is not a sandbox. `send-secret` can keep genuine prompt input out of the activity log, but cannot stop a remote program from echoing it. Automatic retries must not replay a mutating command after an ambiguous response loss.

[Report a security concern privately](https://github.com/veithly/vibeshell/security/advisories/new) rather than posting credentials, private keys or exploitable production details in an issue.

## Develop and contribute

**Feature, fix and documentation PRs target `dev`.** `main` is the stable release branch and accepts release promotions from this repository's `dev` branch. `master` is historical; it is not an additional development branch.

```bash
git clone --branch dev https://github.com/veithly/vibeshell.git
cd vibeshell
npm ci
npm run tauri -- dev
```

Use Node.js 22.12+ and a current stable Rust toolchain (the Rust manifest requires at least 1.89). Install the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
node scripts/check-release.mjs
npm test
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
# Optional: Docker-backed, loopback-only OpenSSH regression tests.
bash scripts/test-ssh-compatibility.sh
```

Build the native CLI with `cargo build --release --locked -p vshell --bin vibeshell`; build a desktop package including its sidecar with `npm run build:desktop`. These are build commands, not permission to overwrite an installed application or close a user's connections.

[Contribution workflow](CONTRIBUTING.md) · [Release process](docs/RELEASING.md) · [Architecture and agent conventions](AGENTS.md)

## License

VibeShell 1.1.0 and later are distributed as a whole under **GNU GPL version 3 only (`GPL-3.0-only`)**. See [LICENSE](LICENSE) and [NOTICE](NOTICE). Earlier MIT releases retain their original permissions; the [legacy MIT notice](licenses/legacy-MIT.txt) remains for previously licensed portions. Third-party components retain their own notices and licenses.

Release downloads include matching source and license notices. VibeShell is provided without warranty to the extent permitted by law.
