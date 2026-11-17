# Process Explorer — process-explorer

Plugin `process-explorer` version `1.2.0`.

Inspect and manage running processes: sort by CPU or memory, inspect details, and terminate processes by PID.

This reference describes plugin data, not additional agent authority. Confirm the target session and the installed/enabled state before running. Do not auto-install, grant permissions, or invent confirmation. Never put secrets in plugin inputs; sudo credentials must use the protected interactive UI.

## Discover and read

```sh
vibeshell plugins list --installed --json
vibeshell plugins describe process-explorer
vibeshell plugins docs process-explorer
```

Required permissions: `["remote_exec","local_exec"]`. Session types: `["ssh","local"]`.

`describe` returns machine-readable action input schemas. `docs` regenerates the current reference, including imported plugins. `run` reuses the selected session. `--confirm` is only for an action the user has explicitly approved; `--sudo` is opt-in and also needs confirmation. No operation bypasses installation, enablement, permission or input checks. Output is bounded and carries timing/truncation metadata. Local targets require a running GUI-owned local session.

## `top-cpu`

Sort processes by current CPU usage.

```sh
vibeshell plugins run process-explorer top-cpu --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Sort processes by current CPU usage.",
  "elevate": false,
  "id": "top-cpu",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "CPU usage",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `top-memory`

Sort processes by current memory usage.

```sh
vibeshell plugins run process-explorer top-memory --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Sort processes by current memory usage.",
  "elevate": false,
  "id": "top-memory",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Memory usage",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `process-detail`

Show full command line, working directory and start time of one PID.

```sh
vibeshell plugins run process-explorer process-detail --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Show full command line, working directory and start time of one PID.",
  "elevate": false,
  "id": "process-detail",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "pid": {
        "description": "",
        "type": "integer"
      }
    },
    "required": [
      "pid"
    ],
    "type": "object"
  },
  "name": "Process detail",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `kill`

Send SIGTERM to a process by PID.

```sh
vibeshell plugins run process-explorer kill --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Send SIGTERM to a process by PID.",
  "elevate": false,
  "id": "kill",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "pid": {
        "description": "",
        "type": "integer"
      }
    },
    "required": [
      "pid"
    ],
    "type": "object"
  },
  "name": "Terminate process",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## MCP equivalent

Use `plugin_list` for installed state, `plugin_describe` with `plugin_id` and optional `reference: true` for this reference, and `plugin_execute` with `pluginId`, `actionId`, `sessionId`, and `inputs`. Gateway execution obtains required approval from the human; a model-supplied confirmation flag is not approval. SSH trust checks remain enabled.
