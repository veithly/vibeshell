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

---

# Task Plan: SFTP Delete, Transfer Progress, and Batch Actions

## Goal
Make SFTP deletion reliable, expose visible upload/download progress, and support practical multi-selection batch workflows across local and SSH sessions.

## Phases
- [x] Phase 1: Establish scope and persistent task records
- [x] Phase 2: Reproduce deletion failure and audit frontend/backend transfer contracts
- [x] Phase 3: Define batch and progress behavior with regression tests
- [x] Phase 4: Implement backend/frontend changes
- [x] Phase 5: Run focused and full verification
- [x] Phase 6: Package, replace, and live-test the installed macOS app
- [x] Phase 7: Review and deliver

## Key Questions
1. Why does the current delete action report unsupported or fail for the user's active session type?
2. Which operations already accept multiple selections, and which are restricted to a single file?
3. Can byte-level progress be emitted by current Rust transfer loops without changing the established IPC boundary?
4. How should partial failures be represented without hiding successfully completed items?

## Decisions Made
- Batch scope: delete, download, compress, extract, select all/clear; upload accepts multiple selected files.
- Progress UI must show operation, active item, completed/total counts, bytes when available, and success/failure outcome.
- Preserve runtime capability gating: native path transfers stay hidden where the platform has no file picker.

## Errors Encountered
- Initial red test used the compact toolbar and did not reach the target handlers; fixed the fixture width so the signal exercises delete/download directly.
- The batch-delete test assumed click order, but the component intentionally processes sorted directory order; compare the complete path set instead.
- TypeScript target does not include `Array.prototype.at`; use indexed access in tests.
- Full `cargo fmt --check` reports unrelated pre-existing formatting changes in plugin files; keep those user-owned edits untouched and verify the SFTP files separately.
- Tauri generated the macOS app successfully but the command ended with an updater-signing error because `TAURI_SIGNING_PRIVATE_KEY` is not configured. The local app bundle was re-signed ad hoc and passed strict verification before installation.

## Status
**Complete** - Implementation, full verification, protected app replacement, and live installed-app QA are finished.
