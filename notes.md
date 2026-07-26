# Notes: Built-in Agent Gateway

## Existing Architecture
- The GUI constructs the shared `Database` and `SessionManager` in `src-tauri/src/lib.rs`.
- The GUI already starts a local socket IPC server used by the CLI.
- The MCP HTTP and stdio servers currently create a separate `SessionManager` from the CLI process.
- The desktop bundle currently includes `vshell` as an external binary and attempts to add it to `PATH`.

## Requirements
- No CLI dependency for agent control.
- Gateway runs as part of the VibeShell GUI and shares GUI sessions.
- Agent-created sessions and actions remain visible to the user in that same GUI.
- If VibeShell is not running, agent instructions launch the visible application and wait for Gateway readiness.
- Agent skill teaches direct Gateway use.
- macOS, Linux, and Windows are supported.
- Implementation is exercised with focused and full project verification.

## Findings
- The current HTTP MCP server binds a fixed loopback port without authentication, discovery, or graceful shutdown.
- The CLI `skill-server` creates a separate `SessionManager`; the GUI Gateway must use the exact Arcs created in `run()`.
- Current MCP `exec` uses an isolated SSH channel and is not rendered in xterm.
- The frontend polls backend sessions every two seconds, so shared-manager sessions appear, but operation visibility needs an explicit activity event/view.
- The existing desktop bundle builds and installs a `vshell` sidecar and mutates PATH; those paths are unnecessary for the Gateway-first Agent integration.
- The generated skill is entirely CLI-oriented and its current install test can target real user directories.
- No single-instance plugin is configured, creating a cold-launch race for multiple Agents.

## Gateway Contract
- HTTP JSON-RPC/MCP on `127.0.0.1:0`.
- `Authorization: Bearer <per-launch-token>` on health and MCP requests.
- Atomic per-user manifest with schema/app/protocol versions, PID, endpoint, token, start time, platform, and executable launch path.
- Visible GUI launch when health is unavailable; no hidden Agent daemon.
- Single GUI state shared by Tauri commands, Gateway tools, session tabs, and Agent activity events.

## Implemented
- Added `mcp/gateway.rs` with loopback dynamic-port hosting, 256-bit rotating bearer tokens, atomic manifests, Unix `0600` permissions, Windows replace semantics, stable launch metadata, graceful shutdown, and status reporting.
- Made the GUI the Gateway session master and added single-instance focus behavior.
- Added authenticated health/MCP routing and removed the CLI `skill-server` entry point.
- Added session reuse plus `session_send_input`, `session_read`, and `session_resize` for shared visible PTY workflows.
- Added a compact Agent activity dock that receives started/succeeded/failed events and immediately refreshes GUI sessions.
- Replaced CLI/PATH settings with Gateway status and removed sidecar/PATH behavior from Tauri and Windows packaging.
- Replaced the generated `vshell` skill with a validated `vibeshell` Gateway skill containing macOS, Linux, Windows launch and discovery flows.
- Updated release assets and README to describe the built-in Gateway rather than a bundled CLI.

## Verification
- `npm run build`: passed.
- `cargo check --workspace`: passed.
- `cargo test --workspace`: 155 passed, 5 ignored.
- `cargo clippy -p vibeshell --lib -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.
- `npx tauri build --no-bundle`: passed; release binary produced.
- Skill Creator `quick_validate.py`: passed.
- Release workflow YAML parse: passed.
- Live Gateway: missing/wrong bearer returned 401; authenticated health returned 200; initialize returned MCP 2024-11-05; tools/list returned 28 tools; session_list succeeded.
- Cold launch: stale endpoint detected, GUI binary launched from manifest, token rotated, authenticated health recovered.
- Single instance: second launch exited 0 while Gateway PID/token remained unchanged.
- Activity lifecycle and Gateway start/stop manifest behavior are covered by isolated Rust tests.

## Residual Platform Coverage
- macOS live and release execution were tested locally.
- Windows and Linux code paths are covered by platform-specific implementation and the existing CI matrix, but cannot be executed on this macOS host because only the Apple Rust target is installed.
- Strict clippy over all targets still reports three unrelated pre-existing lints in `src-tauri/tests/ssh_integration_test.rs`; the application library is clean under `-D warnings`.

---

# Notes: SFTP Delete, Transfer Progress, and Batch Actions

## User-visible Requirements
- Deletion must work instead of reporting unsupported.
- Upload and download need persistent progress feedback.
- Multi-selection must unlock more batch actions.

## Initial Scope
- Batch delete files and directories with partial-failure reporting.
- Batch download selected files and directories to a chosen destination.
- Multi-file upload from one picker action.
- Existing batch compress/extract behavior remains available and becomes clearer in the selection toolbar/menu.
- A transfer status surface stays visible while work is running and summarizes completion.

## Findings
- `sftp_delete` is registered and implemented for local sessions, direct SSH sessions, and remote IPC sessions.
- The frontend discarded `deleteResult.error.message`, reducing every backend failure to a generic item count. This made permission, path, and server capability failures look like an unsupported feature.
- Upload used a single-file picker even though drag-and-drop already iterated multiple paths.
- Download was hard-gated to one selected non-directory file.
- Backend file commands return `TransferProgress` only after completion, so the first reliable UI layer is aggregate per-item progress while a batch is running.

## Red-capable Feedback Loop
- Command: `npm test -- src/components/SftpPanel/__tests__/SftpPanel.interactions.test.tsx`
- Result before the fix: 7 tests, 3 failed.
- Exact failures: delete notification omitted `locked.txt: Permission denied`; batch download exposed no transfer progress; multi-file upload exposed no transfer progress and never called the multi-file picker.

## Implemented
- Added a multi-file native upload picker and registered it with Tauri.
- Added batch upload and batch download for selected files with aggregate progress, current filename, completed/failed totals, and a dismissible final status.
- Added per-file backend error details for delete, upload, and download partial failures.
- Removed the extra remote `STAT` dependency from deletion. Shared direct/IPC deletion now tries file removal and falls back to directory removal/recursive removal.
- Kept folder download out of the supported batch set because there is no recursive remote-to-local download contract yet; selected files are processed, while directory-only selections do not enable Download.

## Targeted Verification
- SFTP interaction tests: 8 passed.
- Frontend production build: passed.
- Rust `cargo check`: passed.

## Full Verification
- Frontend: 29 test files, 108 tests passed.
- Rust: 196 tests passed, 6 ignored.
- Relevant-path `git diff --check`: passed.
- Repository-wide `git diff --check` remains blocked by pre-existing trailing whitespace/CRLF in `src/components/Terminal/Terminal.tsx`.
- Repository-wide Rust formatting remains blocked by pre-existing formatting differences in `src-tauri/src/commands/plugin.rs` and `src-tauri/src/plugins/mod.rs`; no SFTP formatting difference was reported.

## Packaging and Live QA
- Tauri produced `target/release/bundle/macos/VibeShell.app`; updater signing then reported missing `TAURI_SIGNING_PRIVATE_KEY`.
- Re-signed the local bundle ad hoc and passed `codesign --verify --deep --strict`.
- Replaced `/Applications/VibeShell.app` through a temporary backup, verified matching SHA-256, arm64 architecture, bundle identifier, version, and running process, then removed the backup.
- Installed binary SHA-256: `142756662fe53dae51e3031366e1756abbcca07f5fe87f515a5659cf36f58f41`.
- Live UI QA used a local shell only: SFTP listing rendered, single-item menu was complete, Select All selected 127 entries, and the batch menu exposed download for 10 files plus batch compress/delete. No transfer or deletion was executed.
