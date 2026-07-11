# Built-in Agent Gateway Delivery

VibeShell now hosts an authenticated Agent Gateway inside the visible, single-instance desktop GUI. Agents discover an ephemeral loopback MCP endpoint through a user-private manifest, share the GUI's `Database` and `SessionManager`, and can launch the desktop app when it is not running.

The installed `vibeshell` skill uses the Gateway directly and no longer invokes `vshell`, starts a daemon, or depends on `PATH`. Desktop and release packaging no longer include the CLI sidecar.

Users can inspect Agent operations in the new activity dock. Reliable isolated commands remain available through `exec`; shared-terminal workflows use `session_send_input`, `session_read`, and `session_resize`, so commands and output remain visible in the same terminal session.

Verification completed with frontend production build, full workspace tests, strict application-library clippy, release desktop build, skill validation, authenticated live Gateway calls, cold-start recovery, token rotation, and single-instance smoke tests. Windows and Linux execution remains delegated to the repository's existing CI matrix because the local host has only the macOS Rust target.
