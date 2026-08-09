---
name: vibeshell
description: Control SSH and SFTP through VibeShell's built-in Agent Gateway. Use when the user asks to list configured servers, open or reuse visible SSH sessions, run remote commands, inspect logs, transfer or edit remote files, or operate a terminal together with the user in the VibeShell GUI.
---

# VibeShell Agent Gateway

Use VibeShell through its authenticated local MCP Gateway. The Gateway runs inside the visible desktop application and shares the same sessions with the user.

Do not invoke `vshell`, start a headless daemon, open a second session master, or request raw SSH credentials.

## Connect

The Gateway manifest for this installation is:

```text
{{MANIFEST_PATH}}
```

1. Read the manifest without printing or exposing its `token`.
2. If it reports `status: running`, call `GET {endpoint}/health` with `Authorization: Bearer {token}`.
3. If the manifest is missing, stopped, stale, or health fails, launch the visible VibeShell application:
   - macOS: run `/usr/bin/open -a VibeShell`.
   - Linux: run the manifest's `launchPath` as a detached desktop process. If the manifest is missing, try the `vibeshell` desktop binary, `/usr/bin/vibeshell`, or `gtk-launch com.vibeshell.desktop`. Preserve an AppImage path exactly.
   - Windows: use PowerShell `Start-Process -FilePath <launchPath>`. If the manifest is missing, check `%LOCALAPPDATA%\VibeShell\VibeShell.exe` and `%ProgramFiles%\VibeShell\VibeShell.exe`.
4. Poll the manifest for up to 15 seconds. Re-read it after launch because `endpoint` and `token` rotate on every start.
5. POST JSON-RPC requests to `{endpoint}/mcp` with the same Bearer header and `Content-Type: application/json`.

Never put the token in a user-facing response, source file, command log, URL, or environment file. A `401` means the application restarted; re-read the manifest.

## MCP Flow

Initialize, then discover the current schemas instead of guessing arguments:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"agent","version":"1"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

Call a tool with:

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"server_list","arguments":{}}}
```

Tool results are returned in `result.content[0].text`. When `result.isError` is true, treat the text as an operation failure.

## Session Workflow

1. Call `server_list` when the server name or ID is unknown.
2. Call `session_list` and reuse a connected session for the same server when possible.
3. Call `session_create` with `server_name` or `server_id`. It reuses the earliest connected session by default; set `force_new: true` only when the user requests a parallel login.
4. **Default to `session_send_input` for every command the user should see.** Set `append_enter: true`, then use `session_read` to inspect recent terminal output. This is the collaborative path: the command, its output, shell state, and working directory stay in the human's visible terminal.
5. Use `exec` only when the user explicitly asks for an isolated or background command. `exec` runs on a separate SSH channel and its command/output will not appear in the shared terminal.
6. Use named keys such as `enter`, `ctrl-c`, or `escape` for prompts. Do not create another session to answer an existing prompt.

Examples:

```json
{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"session_create","arguments":{"server_name":"prod"}}}
{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"session_send_input","arguments":{"session_id":"<id>","data":"uname -a","append_enter":true}}}
{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"session_read","arguments":{"session_id":"<id>","max_bytes":20000}}}
{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"exec","arguments":{"session_id":"<id>","command":"uname -a"}}}
```

## Remote Files

- Search with `rg` before downloading broad directory trees.
- Read text with `get_content` or `sftp_read`.
- Modify existing text with `edit_file`; prefer exact `old_text` and `new_text` replacements.
- Create text with `add_file`; use `overwrite: true` only deliberately.
- Transfer files with `sftp_upload` and `sftp_download`.
- Use `sftp_upload_directory` for initial directory copies and `sftp_sync_directory` for repeatable deployment syncs.
- Set destructive options such as `delete_extra` or recursive removal only when the user explicitly requests them.

Credentials remain in VibeShell. If `session_create` reports missing credentials or host verification is needed, ask the user to complete that step in the visible VibeShell GUI, then retry the same Gateway call.
