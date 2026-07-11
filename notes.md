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
