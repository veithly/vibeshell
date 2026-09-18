# Changelog

## 1.1.0 — 2026-09-18

### A shared workspace for people and agents

Agent and native CLI commands now have visible operation notifications and durable, encrypted local history. Repeated and multiline commands retain separate records. Agent-created SSH sessions appear as distinct UI tabs without taking focus from a person's active tab.

GUI-first and daemon-first workflows route terminal, SFTP, tunnel, recording and database-probe requests to the process that owns the session. Native plugin discovery, action schemas, live reference documents and guarded execution are available through both CLI and MCP. All 12 built-in plugin references are generated from the same validated manifests used at runtime.

### Safer editing and transfers

Saved passwords, private keys and passphrases can be edited without recreating a server. Omitted fields preserve stored values; metadata and secret changes commit atomically. Credential loading no longer replaces existing key material on corruption or read failures, and first-time key initialization is serialized.

SFTP downloads wait for local file writes before reporting completion. Directory sync detects same-size content changes, protects excluded paths and nested ignore rules from deletion, and rejects overlapping local roots. PTYs can reopen after a remote shell exits; forwarding includes readiness, cancellation and half-close handling.

### UX and packaging

Searchable connection launchers, keyboard focus handling, reduced-motion support, split/detached file workspaces and explicit error states keep terminal work in context. The connection-card animation now uses browser-native APIs; GSAP and its React wrapper have been removed.

The desktop, native CLI, workspace crates, Tauri configuration and agent marketplace metadata share version 1.1.0. English, Simplified Chinese and Japanese READMEs explain workflows and limits rather than claiming universal SSH compatibility.

### Project governance and licensing

The project as a whole moves to **GPL-3.0-only**. Earlier MIT permissions and attribution remain preserved in `licenses/legacy-MIT.txt`; third-party licenses remain their own. Release packaging includes license notices and matching source materials.

`dev` is the default integration branch; `main` is the stable release branch. Feature and fix PRs target `dev`, followed by release-promotion PRs from `dev` to `main`. Publication is tag-driven and stays a draft until validation and all release jobs succeed.

### Not included / compatibility notes

The proposed CLI server-create/delete and Teleport integrations are not merged into 1.1.0; they require changes to preserve credential and file-operation guarantees. Mobile parity, every MFA/hardware-token flow, and every remote plugin dependency are not promised. Remote performance collection assumes Linux `/proc`.

An owner process must remain alive to retain its SSH connections. Upgrade the desktop and CLI together and save ongoing work before restarting either. Existing dependency advisories are not resolved merely by passing functional tests.
