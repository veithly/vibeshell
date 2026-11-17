# Human–Agent collaboration in VibeShell 1.1

[Overview](../README.md) · [简体中文](../README.zh-CN.md) · [Plugin specification](plugin-spec.md)

## Visible operations without interfering with a person

The workspace activity strip shows the most recent Agent/CLI operation. Open it to inspect the full command, session, timestamp and lifecycle state. Independent exec does not inject synthetic text into the person's PTY; its command is visible in history instead.

Native CLI and MCP share persistent local activity records. Each execution has its own identity, so repeating a command does not collapse the history. Full multiline commands are retained; older entries can be paged in. The UI combines event notifications with incremental polling while active and can recover history after restart. Polling is not a promise of zero latency.

`started` means an operation was accepted for processing; `succeeded` means that API operation completed; `failed` includes errors and denied operations. For terminal input, success means delivery, not successful command completion. Split keystrokes remain pending until submitted; submitted commands are recorded separately.

The activity store is locally encrypted and is not cloud-synced. SFTP events record operation/path context, not file contents. Saved credentials and protected stdin are not logged as commands. An ordinary command string can itself contain a secret, so do not place secrets in arguments.

For a genuine password prompt, use the application's protected UI or `vibeshell send-secret SESSION_ID --enter` with protected, piped stdin. The CLI rejects interactive stdin and secret command-line arguments. Redaction follows the pending input line through a later Enter. It cannot prevent a remote program from echoing input, and must not be used to hide commands.

## Shared sessions and tabs

With GUI-first startup, the native CLI service shares the GUI SessionManager. With daemon-first startup, the GUI discovers daemon-owned sessions and keeps existing connections intact. Terminal input, resize, SFTP, tunnels, recordings and database probes route to the process that owns the selected SSH session.

A new live Agent session becomes an independent UI tab without changing the person's active selection. Connections to the same server are distinguished by session identity. History links let the person explicitly switch to the relevant tab.

Connections require their owner process to stay alive: daemon-owned sessions can outlive a GUI window, while GUI-owned sessions end when that process exits. The daemon does not own the GUI's private local shell objects; local plugin actions require the service that actually owns that local session.

If a reply is lost after a command was sent, the client does not automatically replay it. A transport error cannot prove the operation never ran. Retries of mutations need a real idempotency contract, not optimistic assumptions.

## Editing saved credentials

The server edit dialog can change a saved password, replace private-key content/file, and update or clear a key passphrase. Untouched fields preserve stored values. Existing secrets are not loaded into the form. Changing authentication type requires appropriate new credentials.

Metadata, renames and secret updates share a SQLite transaction. Failure rolls back the whole edit; name conflicts do not delete another server's credentials. Closing the dialog clears secret drafts and invalidates pending key-file reads, preventing an old async result from overwriting a new choice.

This changes VibeShell's saved login information, not the remote OS account password. A valid private key also does not establish the server's identity: host-key verification remains necessary.

## AI-readable plugin interface

The interface covers built-ins and validated imported declarative plugins. Server Performance additionally exposes the native `status` action.

```bash
vibeshell plugins list --installed --json
vibeshell plugins describe server-performance
vibeshell plugins docs server-performance
vibeshell plugins run server-performance status --session SESSION_ID --inputs '{}'
```

Use an existing session and check enabled state before running. Without `--installed`, the list includes uninstalled catalog entries. Descriptions expose action input schemas, permission requirements, confirmation and sudo capabilities, but not saved settings or credentials.

| MCP tool | Parameters | Result |
| --- | --- | --- |
| `plugin_list` | `installed_only` (default true) | Current install/enabled state and documentation entry points |
| `plugin_describe` | `plugin_id`, `reference` | Action schemas; `reference: true` returns Markdown |
| `plugin_execute` | `pluginId`, `actionId`, `sessionId`, `inputs`, optional `trySudo` | Bounded output, duration and truncation state, or an error |

Execution checks installed/enabled state, granted and declared permissions, session type and inputs. It never installs or enables a plugin as a side effect. CLI `--confirm` is only appropriate after the person approves the exact operation; sudo is a separate explicit opt-in. MCP obtains approval from the human gateway, and rechecks that the command has not changed during review.

Nonzero command exits are errors. SSH output and runtime are bounded; the plugin then applies its UTF-8-safe output cap. It does not use a `head` pipeline that would hide the command's exit status. Local execution drains stdout/stderr concurrently, closes unused stdin and bounds captured output and time.

External tools and permissions remain the environment's responsibility. A Docker, Kubernetes or database manifest does not install its server or client program. Remote performance collection currently assumes Linux `/proc`.

## Main Skill and reference documents

The main Skill contains discovery commands, shared safety rules and a compact built-in index. Detailed usage belongs in `references/<plugin-id>.md`. The repository generator and native installer use the same validated manifests and reference renderer; references are checked in beside all three Skill copies for direct GitHub/plugin installation.

Regenerate repository references with `cargo run --locked -p vibeshell-plugins --example export_references`; add `-- --check` in CI. This touches no user directories or live sessions. The application installer writes reference files before the main Skill and is idempotent for unchanged content.

After importing or updating a plugin, `vibeshell plugins docs <id>` is authoritative for the running installation. Static documents are a snapshot, not evidence a plugin is currently enabled. Documentation never grants authority or overrides the user's instructions.

## Validation boundaries

Regression tests cover repeated/multiline command records, pagination, tab focus, atomic credential edits, stale async key reads, plugin permissions and reference/catalog parity. The isolated OpenSSH fixture covers common authentication, PTY, SFTP, forwarding and jump-host behavior.

These checks do not prove every external server, plugin dependency, MFA flow or platform works. UI interaction, binary packaging, signature validation and installed-app behavior require separate verification. Keep desktop, service/CLI and Skill versions aligned during upgrades, and preserve active work before restarting an owner process.
