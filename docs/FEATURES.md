# VibeShell feature reference

[English overview](../README.md) · [简体中文](../README.zh-CN.md) · [日本語](../README.ja.md)

This is an implementation-backed inventory of the current source tree, including the glass-theme workspace changes. It is **not** a release compatibility certificate or an end-to-end test of every action. A published binary may predate this tree. Use its matching tag and release notes when investigating a particular installation.

[Connections](#connections) · [Sessions and CLI](#sessions-and-cli) · [AI](#ai-and-collaboration) · [Files](#files-and-transfers) · [Plugins](#operations-plugins) · [Appearance](#appearance-and-layout) · [Tunnels and recording](#tunnels-snippets-and-recording) · [Data](#data-and-security) · [Platforms](#platforms-and-development)

## Connections

### Profiles and authentication

The connection launcher combines saved SSH hosts, detected local shells and supported coding-agent launchers. Saved servers have names, address/port, username, authentication type, groups, tags and optional jump-host/post-login settings. Search and list/card presentation help navigate existing profiles. Returning to an active connection and explicitly creating another session are separate actions.

Password and OpenSSH private-key authentication are implemented, including encrypted private keys with a passphrase. Common password-style keyboard-interactive authentication has a bounded fallback; arbitrary MFA conversations are not universally supported. Host-key trust is checked before authentication, including the actual destination reached through a jump host. Changed keys are not silently accepted.

Saved password/key/passphrase changes preserve untouched fields and commit with profile metadata. Existing secrets are not populated into the editing form. Changing saved login information does not change the account password on the server.

**ForwardAgent boundary:** imports and the profile schema retain `agent_forwarding` metadata. That alone does not establish a working SSH-agent forwarding channel; do not infer agent socket relay or hardware-token support from the stored field. Post-login commands, by contrast, are used by session creation and should be reviewed as executable content.

Implementation: [server commands](../src-tauri/src/commands/server.rs), [session manager](../src-tauri/src/session/manager.rs), [SSH client](../src-tauri/src/ssh/client.rs), [host fingerprints](../src-tauri/src/ssh/fingerprint.rs), [connection launcher](../src/components/SelectServerDialog/SelectServerDialog.tsx).

### Importing existing configurations

`vibeshell import` supports `auto`, `openssh`, `putty` and `tabby`, optional `--path`, preview-only `--dry-run` and structured `--json` output. OpenSSH parsing covers fields such as `Host`, `Include`, `IdentityFile`, `ProxyJump`, `RemoteCommand` and `ForwardAgent` metadata. It is an importer, not a complete OpenSSH configuration interpreter.

Third-party stored passwords are not copied. Imported private-key paths reference local files; PuTTY `.ppk` keys require conversion to OpenSSH format. Review imported hosts and executable post-login settings before connecting. Use the desktop to add/edit/delete saved servers; the current CLI offers listing/import, not saved-server CRUD commands.

Implementation: [importers](../src-tauri/src/ssh_import), [CLI parser](../cli/src/main.rs), [CLI installation guide](../cli/README.md).

## Sessions and CLI

### Ownership and execution context

| Surface | Execution context | Important consequence |
| --- | --- | --- |
| GUI terminal input | Selected session's interactive PTY | The user changes that shell's working directory and environment |
| Native `ssh`, `ssh-session`, `exec` | The same interactive PTY | Subsequent CLI/GUI commands share `cd`, `export` and interactive process state |
| Independent MCP execution and plugin actions | Separate execution path/channel | Do not assume the interactive PTY's current directory or shell variables apply |
| Coding-agent launcher | A local PTY in the selected project | A local coding task is not automatically a remote SSH task |

By default `ssh SERVER` reuses the earliest active session for that saved server; `--new` requests another. `sessions` reports short aliases as well as session identities. Multiple connections to the same host are not collapsed into one server-name tab.

The GUI can own the local IPC service or discover a daemon-owned service. Requests for terminal input, SFTP and related session operations route to the owning process. Daemon-owned sessions can survive closing the GUI; GUI-owned connections end when that owner exits. Detached windows reattach to the existing session rather than creating a new SSH login. Local shell ownership is separate from the daemon's SSH-session map.

When the shell emits OSC 3008 completion events, the CLI returns the command's remote exit code. Otherwise it reports that exit status could not be verified. A prompt handoff or delivered keystroke does not establish completed execution. Lost replies do not safely authorize automatic replay of a mutation.

Implementation: [native CLI commands](../cli/src/commands), [IPC routing](../src-tauri/src/ipc/runtime_services.rs), [session store](../src/stores/sessionStore.ts), [shared-session regression cases](../src-tauri/src/ipc/shared_session_test.rs).

### Command map

| Commands | Purpose |
| --- | --- |
| `version`, `license` | Inspect the binary version and license/source notices |
| `servers`, `import` | List saved targets and import existing profiles |
| `ssh`, `ssh-session`, `attach`, `exec` | Connect, reuse a session, attach interactively or submit a command |
| `sessions`, `kill` | Discover live reusable sessions and deliberately terminate them |
| `send-key`, `send-secret` | Deliver named keys/literal input or protected secret stdin |
| `sftp` | Direct or interactive file operations |
| `rg` | Remote search, using ripgrep with grep fallback |
| `get-content`, `add-file`, `edit-file` | Read, create or update remote text through SFTP |
| `plugins list`, `describe`, `docs`, `run` | Discover installed state, schemas, references and execute a reviewed action |
| `daemon` | Start or inspect the headless service |
| `tools`, `install`, `uninstall` | Inspect coding-tool integrations and manage the bundled Skill |

Use `--help` on the installed binary for exact flags and aliases. `--command-file` and `--command-stdin` avoid nested local-shell quoting for remote commands; file-editing commands have separate `--content-file`, `--content-stdin` and content/replacement options. These are not interchangeable flags.

`add-file` refuses an existing destination unless overwrite is explicitly selected. `edit-file` can replace the entire content or perform an exact text replacement. Review both the current file and the proposed content before writes. `send-secret` takes stdin rather than secrets on the command line; it cannot prevent the remote process from echoing them.

Canonical instructions: [native CLI Skill](../skills/vibeshell/SKILL.md), [command definitions](../cli/src/main.rs), [command-input handling](../cli/src/command_input.rs), [remote file tools](../cli/src/commands/file_tools.rs).

### Terminal ergonomics

xterm.js supplies the terminal, with a WebGL renderer when available and batched input/output handling. Settings expose font family/size, cursor style/blink and scrollback. Local completion uses command/subcommand/flag definitions and history; suggestions can be selected with the keyboard. Reusable command snippets and saved history complement ordinary shell history. Rendering choices do not imply lower SSH network latency.

Implementation: [Terminal](../src/components/Terminal/Terminal.tsx), [completion](../src/components/Terminal/useCompletion.ts), [settings](../src/stores/settingsStore.ts), [command history](../src/stores/commandHistoryStore.ts).

## AI and collaboration

### Three distinct integrations

| Integration | What VibeShell does | What the user supplies |
| --- | --- | --- |
| Coding-agent terminal | Detects and launches Claude Code, Codex, OpenCode or Pi in a local project; shows Git status/diffs | Installed tool, authentication, model access and reviewed launch mode |
| Native agent Skill / MCP gateway | Exposes saved targets, session/file workflows and plugin discovery/execution | An external agent with explicitly authorized access |
| Optional command prediction | Sends context to a configured OpenAI-compatible or Claude endpoint and displays a suffix | API endpoint, model and API key; deliberate opt-in |

Start modes are tool-specific. Claude Code and Codex have new/continue/resume choices; OpenCode has new/continue without a resume picker; Pi's supported access modes differ from the others. The launcher validates supported combinations rather than assuming identical flags. Workspace changes reads local Git branch/status and per-file diff; it does not independently prove a generated patch is correct.

Prediction is disabled by default. Its request includes current input, recent history and local completion candidates. Suggestions are not automatically executed. Its API key is persisted in the app-settings JSON and the localStorage fallback, not in the encrypted SSH credential store. Disabling prediction does not disable local completion.

Implementation: [agent launch adapters](../src-tauri/src/coding_agent/launcher.rs), [launcher UI](../src/components/CodingAgentLauncher/CodingAgentLauncher.tsx), [Git review](../src/components/WorkspaceChangesPanel/WorkspaceChangesPanel.tsx), [prediction request](../src/lib/aiCommandPrediction.ts), [settings persistence](../src-tauri/src/commands/settings.rs).

### Activity, approval and discovery

The History panel identifies user and CLI-originated operations with different colors and includes their session. New CLI-created sessions appear without forcing a tab switch. Repeated commands, multiline text, incremental refresh and older-history pagination are supported. Automatic host telemetry is not displayed as Agent work. SFTP activity records operation/path context, not file contents.

Guarded agent requests are reviewed through an approval dialog. Settings include the guard switch, independent-exec coverage and custom force-review/allow patterns. Broad allow patterns or temporary automatic approval change the protection level. Plugin execution also checks installed/enabled state, declared/granted permissions, session type, input schema, confirmation and sudo eligibility. Reading a reference does not install or authorize a plugin.

The desktop gateway binds to loopback and uses an authentication token. Its local discovery manifest contains sensitive connection material. Do not paste it into public documentation, expose the gateway to a network, or replace token-based access with an unauthenticated endpoint. The installed native CLI Skill remains the straightforward agent workflow.

Approval classification is not a sandbox, malware detector or universal proof of safety. Submission and completion are different history states. Sensitive terminal output can still be recorded or displayed if a remote program echoes it.

Implementation: [activity UI](../src/components/AgentActivityPanel/AgentActivityPanel.tsx), [approval UI](../src/components/AgentApprovalDialog/AgentApprovalDialog.tsx), [guard](../src-tauri/src/mcp/guard.rs), [gateway](../src-tauri/src/mcp/gateway.rs), [collaboration guide](AGENT_COLLABORATION.md).

## Files and transfers

### Browsing and mutation

The file-management panel uses SFTP for SSH sessions and local file operations for local sessions. It offers details, column and icon views; breadcrumb navigation; home/up/refresh; multiple selection; copy-path actions; folder creation; rename/delete; file/folder upload; download; synchronization; and compression/extraction actions. Destructive operations and remote archive tooling still require a suitable target and permission.

The transfer path uses bounded chunks and reports completion after local download writes finish. Recursive upload/sync supports exclusion patterns and nested `.gitignore`. Same-size edits can require content comparison and additional reads. `--delete` deliberately removes remote extras while respecting protected exclusions; it is not a backup strategy or an operation to invoke on an unreviewed directory.

Interactive CLI SFTP supports `pwd`, `ls`, `cd`, `get`, `put`, `sync`, `cat`, `mkdir`, `rm`, `mv`, `help` and `quit`. Direct CLI operations can select an existing session with `--session`. Local UI file browsing and remote SFTP share presentation but do not have identical transport semantics.

Implementation: [SFTP panel](../src/components/SftpPanel/SftpPanel.tsx), [SFTP backend](../src-tauri/src/sftp), [sync logic](../src-tauri/src/sftp/sync.rs), [CLI SFTP](../cli/src/commands).

### Direct editing, archive preview and extraction

Double-click a remote text file to open its editor tab and save with **⌘/Ctrl+S**. Markdown supports source, rendered preview and a side-by-side view. Images, PDF and media use their respective viewers. These are document actions, separate from uploading and downloading a file.

Select a remote archive and choose **Extract here** from the SFTP context menu, or use the extract toolbar action. Extraction runs on the SSH host in the current directory using its installed tools: `tar` for TAR/TAR.GZ/TAR.BZ2/TAR.XZ (including TGZ/TBZ2/TXZ), `unzip` for ZIP, `gunzip`/`bunzip2`/`xz` for individual compressed files, and `7z`/`unrar` for their corresponding formats. Multiple selected archives are processed in sequence. Compression offers `.tar.gz` and `.zip` for selected files in one directory.

Extraction can overwrite existing files (ZIP uses `unzip -o`); inspect the destination and use trusted archives. Archive actions are currently implemented for SSH sessions, not local-file sessions. Opening an archive tab only lists supported entries—it does not execute the remote extraction action. Preview supports a narrower set of formats than extraction, as listed below.

Implementation: [archive command construction and handlers](../src-tauri/src/commands/sftp.rs), [selection and context-menu actions](../src/components/SftpPanel/SftpPanel.tsx).

### Document workspace

Local files open via **⌘/Ctrl+O**, the native **Workspace → Open Files** menu, or supported packaged macOS file associations. The application registers as an alternate opener; it does not claim to replace the system's default editor. Finder drag-in is not implemented by this workflow.

Files have independent workspace tabs. Closing the last terminal does not close local documents. Unsaved buffers follow file-pane moves, split layouts and detached windows; moving a tab is not an implicit file save.

| Content | Current behavior and bounds |
| --- | --- |
| UTF-8 text and code | Editing and syntax highlighting; local text limit 4 MiB; truncated reads are not offered as complete-file saves |
| Markdown / `.mdx` source | Basic source, preview or split view; not a full CommonMark/GFM implementation, and no MDX/JavaScript execution |
| Markdown images | External images blocked until explicitly enabled; local relative images limited to 8 MiB; preview limited to 512 Ki characters |
| Images | Common formats, scaling/rotation/fit; decoding depends on WebView support |
| PDF / audio / video | Embedded platform-supported preview; no universal format or codec guarantee |
| Archive inventory | ZIP, TAR and TAR.GZ/TGZ listing/search; input preview limit 32 MiB; up to 5,000 entries; expanded TAR cap 128 MiB; no ZIP64 |
| Other binary preview | Local binary read cap 64 MiB; unsupported content produces a clear fallback |

Local text writes compare the loaded original to detect external changes, preserve the editing draft on conflict and use a temporary file in the same directory. This is optimistic conflict detection, not an exclusive cross-program lock. Invalid UTF-8, UTF-16 or binary/NUL content is not silently rewritten as lossy text. Do not assume the same local conflict detector applies to remote SFTP writes.

Markdown does not execute raw document HTML or scripts. External links require explicit navigation; relative-image restrictions are not a comprehensive filesystem symlink sandbox. Large or unsupported documents should be opened with a specialized viewer.

Implementation: [file workspace](../src/components/FileWorkspace/FileWorkspace.tsx), [local file commands](../src-tauri/src/commands/local_files.rs), [format/size definitions](../src/lib/fileWorkspace.ts), [archive parser](../src/lib/archivePreview.ts), [Markdown renderer](../src/components/FileWorkspace/MarkdownPreview.tsx), [document behavior guide](local-files-and-css-themes.md).

## Operations plugins

All twelve built-ins declare SSH and local session support, but a compatible session type does **not** establish OS/tool availability. Most command plugins depend on Unix/Linux tooling. The links below point to generated references; the running installation's `plugins docs ID` is authoritative after an update or external import.

| Plugin ID and reference | Implemented action families | Environment boundary |
| --- | --- | --- |
| [server-performance](../skills/vibeshell/references/server-performance.md) | Native `status`: CPU/memory/disk/network/load/uptime fields | Remote Linux `/proc` and common host commands; local metrics are partial |
| [docker-containers](../skills/vibeshell/references/docker-containers.md) | Running/exited/all inventory, stats, images, logs, inspect, exec, start/stop/restart, volumes/networks/version | Docker CLI and daemon access; exec is one-shot, not a persistent container shell |
| [kubernetes-pods](../skills/vibeshell/references/kubernetes-pods.md) | Contexts, pods, deployments, events, logs, describe, exec | `kubectl`, cluster context and RBAC; no promise of full cluster administration |
| [database-inspector](../skills/vibeshell/references/database-inspector.md) | PostgreSQL/MySQL databases/tables/queries; SQLite discovery/queries; table-oriented output | Appropriate clients/access; PostgreSQL/MySQL may use an available container client fallback |
| [redis-inspector](../skills/vibeshell/references/redis-inspector.md) | Ping/info/keyspace/size, scan/get/type/TTL/delete, clients, slowlog, big keys, config lookup | `redis-cli` or supported Docker fallback; deletion is a mutation |
| [process-explorer](../skills/vibeshell/references/process-explorer.md) | CPU/memory ordering, PID details, termination | `ps`, shell utilities and `kill`; permission and process ownership apply |
| [system-logs](../skills/vibeshell/references/system-logs.md) | Recent journal, service journal | `journalctl` and permission to read the journal |
| [network-inspector](../skills/vibeshell/references/network-inspector.md) | Sockets, routes, addresses, DNS | Linux-oriented `ss`, `ip`, `resolvectl` |
| [disk-usage](../skills/vibeshell/references/disk-usage.md) | Filesystem and directory usage | `df`, `du`, access to the requested paths |
| [git-workspace](../skills/vibeshell/references/git-workspace.md) | Status, branches, history | Git and a repository in the action's actual execution context |
| [cron-scheduler](../skills/vibeshell/references/cron-scheduler.md) | User crontab, system cron files, systemd timers, add/remove user entries | `crontab`, shell tools and, for timers, systemd; writes need deliberate approval |
| [systemd-services](../skills/vibeshell/references/systemd-services.md) | List/status/start/stop/restart | `systemctl` and service-management permission; sudo is not automatic |

Database dashboards additionally manage PostgreSQL/MySQL connection profiles and connection probes. They are not a general GUI migration suite. SQLite is reached through its declared CLI actions. Read the action schema before passing query text or credentials; a dashboard does not make arbitrary SQL read-only.

**Performance coverage:** the shortcut opens a bottom dock, follows the current session and supports resizing, collapse, close and refresh interval selection. Remote Linux collection reads `/proc`, `df`, `hostname` and related data. Local collection obtains available cores/load/memory/disk information using OS-specific tools; current local CPU usage and uptime retain default values and local network data is empty. Windows fallback depends on WMIC availability. A displayed zero is not evidence of an idle host when the collector does not implement that field.

### Extension model

A plugin is a validated, declarative JSON manifest, not a loaded JavaScript/WASM/native extension. The host renders its interface, validates arguments and captures bounded output from declared programs. The v1 format has no remote registry, package-signing system or automatic plugin update channel. Import/export is local-file based. Installation does not install Docker, database clients or any other remote dependency.

The CLI/MCP discovery API reports installed/enabled state, action input schemas and current reference documents. Execution checks declared/granted permissions and supports explicit confirmation and sudo opt-in where declared. Documentation is descriptive, not authorization. External plugin settings must not contain secrets, especially because plugin installations/settings participate in portable synchronization.

Implementation: [built-in manifests](../plugins/builtin), [normative plugin spec](plugin-spec.md), [plugin development](plugin-development.md), [CLI interface](../cli/src/commands/plugins.rs), [native status collection](../src-tauri/src/commands/session.rs), [database connection backend](../src-tauri/src/dbconn), [bottom dock](../src/components/ServerStatus/PerformanceDock.tsx).

## Appearance and layout

### Current interface

The workspace uses a single title/tab bar. SFTP, performance, history and more are icon-only controls with tooltips, accessible names and pressed/disabled/focus states. No logo/session-summary strip or separate row of split buttons remains. A single terminal has no outer padding or duplicate pane title.

On macOS, the transparent native window and AppKit material sample the **real content behind the window**. CSS supplies neutral tints, not a decorative gradient wallpaper. Terminal background tint defaults to 90%; window chrome/panel tint defaults to 12%. These affect background layers, not overall window or text opacity. Tabs remain opaque. Other hosts fall back to an ordinary solid background, and reduced-transparency preferences override translucency.

Settings centralizes day/night/system appearance, five color presets, fonts, cursor behavior and opacity controls. Appearance is the first section; navigation jumps to major settings groups, and custom CSS is an advanced disclosure. Settings forms, plugin panels, file views, menus/dialogs and detached windows share neutral material tokens. Reduced-motion behavior is supported.

Custom CSS can change colors, spacing, typography, corners and backgrounds. Preview is temporary; apply/save persists and synchronizes within the application's windows. Import fills the editor without automatically enabling it. CSS can make network requests or hide controls, so only apply trusted themes. **⌘/Ctrl+Shift+F12** and the native **Workspace → Disable Custom CSS** recovery entry remain available. Embedded local image size is limited to 1 MiB; CSS size to about 2 MiB.

Implementation: [glass material styles](../src/glass.css), [theme synchronization](../src/lib/useThemeSync.ts), [macOS window config](../src-tauri/tauri.macos.conf.json), [appearance settings](../src/components/Settings/AppearanceModeControl.tsx), [custom theme implementation](../src/lib/customTheme.ts), [starter CSS](../themes/vibecode-starter.css).

### Tabs, panes and native windows

Terminal, file and plugin tabs can be reordered, moved to pane edges, split and detached. The maximum layout is nine panes. A detached handoff removes the source only after the receiving window acknowledges it. Closing a detached window returns its content; terminating a terminal session is a different action and requires confirmation. Return-button behavior is available even where native cross-window mouse-release docking is unsupported.

Layouts save tab order/selection, split tree/ratios, detached targets and window geometry. Restored windows are clamped to available displays. Reopening a layout may recreate local shells or require reconnecting SSH; it does not resume arbitrary terminated programs or automatically rerun finished agents. Dirty document recovery is separate from saving the actual file.

Implementation: [window guide](window-management.md), [detach/handoff](../src/lib/detach.ts), [persistence](../src/lib/workspacePersistence.ts), [file buffers](../src/lib/fileEditBuffer.ts), [pane tree](../src/lib/mosaicTree.ts).

## Tunnels, snippets and recording

Saved tunnel configurations cover local forwarding, reverse forwarding and dynamic SOCKS5. Start/stop actions are associated with a selected SSH session and routed to its owner. Binding to an address other than loopback can expose a forwarded service; review listen addresses and remote authorization. A local SOCKS endpoint is not configuration for routing VibeShell's own SSH connection through an outbound proxy.

Command snippets support reusable commands, categories, descriptions, tags, search and editing. They are executable text and may expose secrets if copied into an export; keep credentials out of snippets. The native command Skill's file and command examples are not automatic approval to execute them.

Recording captures SSH terminal output as local `.log` bytes, with recording metadata/listing/deletion in Settings. It is **not** a timed asciicast or video format; there is no built-in playback interface. The current documentation does not claim local-shell recording parity. Unix permissions restrict created session directories/files to the user, but output logs are not encrypted and can contain sensitive remote output. Session termination also triggers associated cleanup.

Implementation: [tunnels](../src-tauri/src/tunnel), [tunnel commands](../src-tauri/src/commands/tunnel.rs), [snippet manager](../src/components/SnippetManager/SnippetManager.tsx), [recording implementation](../src-tauri/src/logging/session_logger.rs), [recording commands](../src-tauri/src/commands/logging.rs).

## Data and security

### Storage is not one blanket encryption claim

| Data | Current handling | What not to assume |
| --- | --- | --- |
| Saved SSH credentials | AES-256-GCM encrypted local values with device-local key material | Not OS Keychain custody; not safe from a compromised local account |
| Agent/CLI activity | Encrypted, persistent local activity records | Command arguments may still contain user-supplied secrets; not cloud-synced |
| Prediction API key / general app settings | Persisted settings JSON and browser-storage fallback | Not the encrypted SSH credential store |
| SSH host fingerprints | Device-local trust records, checked before authentication | Not a substitute for checking an unexpected new key; not synchronized |
| Terminal recordings | Raw local output logs | No encryption or automatic removal of secrets printed by a remote program |
| Unsaved document buffers | Local recovery data during editing/window moves | Not an encrypted vault and not the saved remote file |
| Portable exports | Versioned plain JSON | Not the encrypted Gist/WebDAV transport envelope |
| Gateway discovery / vault pairing material | Local access/connection material | Must not be published, logged into issues or treated as harmless settings |

Encrypted storage is an at-rest protection with local key files, not a threat boundary against malicious code running as the same user. Plugin confirmation is an operation policy, not a sandbox. Secrets should not be passed in CLI arguments or normal terminal commands. Protected input redaction cannot suppress output produced by a remote program.

Implementation: [credential crypto](../src-tauri/src/storage/crypto.rs), [database](../src-tauri/src/storage/database.rs), [settings commands](../src-tauri/src/commands/settings.rs), [activity implementation](../src-tauri/src/mcp), [file buffers](../src/lib/fileEditBuffer.ts).

### Optional sync and portable backup

Gist and WebDAV providers transport encrypted batches for four entity types: **servers, groups, command snippets and plugin installations**. This is user-configured synchronization, not a VibeShell account requirement, hosted SSH proxy or live terminal relay. Credentials, host-key trust, activity, recordings, current terminal processes, and general app/CSS settings are not those four synchronized entity types.

Portable file export/import carries a versioned JSON snapshot with a 16 MiB bound. Import merges supported records and rejects deletion records rather than treating an imported snapshot as permission to delete local data. This format is unencrypted and may include hostnames, commands, plugin manifests and non-secret plugin settings. Review and protect it before sharing.

Provider tokens, pairing codes and vault key material are sensitive. Importing a plugin does not waive its validation or executable-command permissions. Sync conflict resolution and device-local credentials remain separate concerns.

Implementation: [sync entity schema](../src-tauri/src/storage/sync.rs), [encrypted envelopes](../src-tauri/src/storage/sync_crypto.rs), [providers](../src-tauri/src/cloud_sync/providers.rs), [portable JSON](../src-tauri/src/cloud_sync/portable_file.rs), [cloud sync UI/store](../src/stores/cloudSyncStore.ts).

## Platforms and development

Desktop targets are macOS, Windows and Linux. Published package formats and architectures are described in the README and each release's assets. Native behind-window material in this redesign is macOS-specific; other platforms share the visual system without claiming that effect. The UI languages are English and Simplified Chinese; Japanese is a README translation.

iOS/Android are experimental, foreground-oriented SSH/SFTP targets with host-key verification and pasted-key workflows. Backend capabilities disable desktop-only window controls, local shells/coding agents, updater/process integration, CLI IPC, directory transfer and background tunnels. Mobile sockets are not promised to survive OS suspension. See [mobile development](mobile-development.md) for signing/toolchain requirements and file-adapter boundaries.

Use Node.js 22.12+, stable Rust and Tauri's platform prerequisites. `npm run tauri -- dev` runs the native application; `npm run dev` alone is only the frontend. `npm run build:desktop` prepares the native CLI sidecar before desktop packaging. A successful local build neither proves every platform works nor authorizes replacing an installed app.

Normal code and documentation PRs target `dev`. CI compilation and releases are manual `workflow_dispatch` workflows; required checks still gate integration. Tags and pushes are not release triggers. Test data must be temporary and isolated from the user's real servers, keys and database.

Implementation and workflow: [capability contract](../src-tauri/src/platform/capabilities.rs), [package scripts](../package.json), [contributing](../CONTRIBUTING.md), [release process](RELEASING.md), [source architecture](../AGENTS.md).

### Explicit non-claims

VibeShell does not currently promise universal MFA/hardware-token/Teleport support, a persistent Docker/Kubernetes shell, a remote plugin registry, native translucent material on every platform, built-in terminal recording playback, encryption of all settings/files, a live-session cloud backup, or full metric parity across local operating systems. These limits are part of the feature description, not hidden behind screenshots.
