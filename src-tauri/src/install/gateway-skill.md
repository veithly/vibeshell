---
name: vibeshell
description: Control SSH and SFTP through VibeShell's visible Agent Gateway. Use for shared terminal sessions, remote commands, logs, files, and transfers.
---

# VibeShell Gateway

Use the bundled `gateway.mjs` next to this file. It discovers the local
Gateway, starts the visible VibeShell app when needed, authenticates without
exposing the token, and handles MCP setup automatically.

```text
node gateway.mjs list
node gateway.mjs connect <server-name-or-id>
node gateway.mjs send <session-ref> "uname -a"
node gateway.mjs read <session-ref>
node gateway.mjs call <tool> '{"...":"..."}'
```

The output uses short, unambiguous server/session references. Full UUIDs remain
accepted when a tool is called directly. Do not invoke `vshell`, start a
headless daemon, open a second session master, or request raw SSH credentials.

For commands the user should see, always use `send` (MCP `session_send_input`)
so the command, output, shell state, and working directory stay in the visible
terminal. Use `exec` only when the user explicitly asks for isolated/background
execution. Risky commands fail closed until the user approves them in the
VibeShell dialog; credentials remain in the GUI.

Use `call` for SFTP and other tools. Prefer `rg` before broad downloads,
`get_content`/`sftp_read` for text, `edit_file` for exact replacements, and
explicit destructive options only when the user requests them.

Common tools: `server_list`, `session_create`, `session_send_input`,
`session_read`, `exec`, `sftp_upload_directory`, `sftp_sync_directory`,
`get_content`, `edit_file`, `add_file`.
