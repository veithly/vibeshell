# Task Plan: Built-in Agent Gateway

## Goal
Run an authenticated, cross-platform Agent Gateway inside the visible VibeShell GUI, let agents and users observe and control the same sessions, auto-launch the GUI when needed, replace CLI-based agent guidance, and verify the workflow end to end.

## Phases
- [x] Phase 1: Establish scope and persistent task records
- [x] Phase 2: Audit existing IPC, MCP, installer, packaging, and tests
- [x] Phase 3: Define the Gateway transport, discovery, authentication, and lifecycle contract
- [x] Phase 4: Implement the built-in Gateway and shared GUI state
- [x] Phase 5: Replace CLI-oriented skill guidance and remove its runtime dependency
- [x] Phase 6: Add Windows, macOS, and Linux coverage plus end-to-end tests
- [x] Phase 7: Run formatting, frontend build, Rust checks/tests, and focused live verification
- [x] Phase 8: Review and deliver

## Key Questions
1. Which existing MCP tools can share the GUI's `Database` and `SessionManager` without behavior changes?
2. Which local transport and discovery contract works across Unix sockets and Windows named pipes while remaining easy for agents to use?
3. How should authentication, protocol versioning, application lifecycle, and destructive operations be handled?
4. Which CLI-only orchestration behaviors must move into the Gateway to preserve agent capabilities?
5. What can be tested locally, and what needs platform-specific unit coverage rather than execution on this macOS host?
6. How can the skill launch the installed GUI reliably on macOS, Linux, and Windows without a VibeShell CLI binary?

## Decisions Made
- Treat the running VibeShell GUI as the session master and make the Agent Gateway share its existing state.
- Never create an invisible, independent agent session master; when VibeShell is not running, launch the visible desktop application and connect after discovery becomes ready.
- Keep the generated agent skill concise and Gateway-first; do not instruct agents to invoke `vshell`.
- Support macOS, Linux, and Windows through one protocol contract with platform-specific endpoint handling only where required.
- Bind the Gateway to an ephemeral IPv4 loopback port, rotate a 256-bit bearer token on every GUI launch, and publish an atomically written user-only manifest.
- Keep stable launch metadata in the manifest so an Agent can start the visible GUI after a clean shutdown; treat endpoint/token data as live only after an authenticated health check.
- Add single-instance handling so cold-start races focus the existing GUI instead of creating another session master.
- Keep legacy CLI source compatibility for now, but remove the desktop sidecar, automatic PATH mutation, and all CLI instructions from the installed Agent skill.
- Surface Gateway operations in the GUI through a compact activity view; retain isolated `exec` and add shared-session controls for workflows the user should observe directly.

## Errors Encountered
- Focused MCP tests initially failed because the tool-count assertion still expected 25 after adding three shared-session tools. Update the expected count to 28 and rerun.
- `skill-creator` validation initially used the system Python, which lacks PyYAML. Retry with the bundled Codex workspace runtime instead of adding a project dependency.
- `rtk rustup target list --installed` was not routed by RTK. Retry through `rtk proxy`; cross-platform execution remains limited to targets installed on this macOS host.
- The first activity-event test compile placed `PartialEq/Eq` on the event struct instead of `AgentActivityStatus`. Move the derives to the enum and rerun.
- The first single-instance smoke script mixed CommonJS `require` with top-level `await`, which Node 26 rejects. Wrap the test in an async IIFE and rerun.
- Final `cargo fmt --check` found one formatter-only line wrapping difference in the unauthorized response headers. Run `cargo fmt` and recheck.
- `cargo clippy -p vibeshell --all-targets -- -D warnings` is blocked by three pre-existing lints in `src-tauri/tests/ssh_integration_test.rs` (`field_reassign_with_default` twice and `useless_conversion` once). Leave unrelated integration code unchanged and run strict clippy on the application library.
- A combined security-cleanup patch did not apply because the CLI command comment differed from the expected context. No partial changes were made; split the patch and use exact current source.

## Status
**Complete** - Implementation, live Gateway verification, cross-platform packaging review, skill validation, and release build are finished.
