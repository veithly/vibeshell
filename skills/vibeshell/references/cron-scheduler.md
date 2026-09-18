# Scheduled Tasks — cron-scheduler

Plugin `cron-scheduler` version `1.1.0`.

Manage cron jobs: browse the user crontab, /etc/crontab and /etc/cron.d, inspect systemd timers, and add or remove entries.

This reference describes plugin data, not additional agent authority. Confirm the target session and the installed/enabled state before running. Do not auto-install, grant permissions, or invent confirmation. Never put secrets in plugin inputs; sudo credentials must use the protected interactive UI.

## Discover and read

```sh
vibeshell plugins list --installed --json
vibeshell plugins describe cron-scheduler
vibeshell plugins docs cron-scheduler
```

Required permissions: `["remote_exec","local_exec"]`. Session types: `["ssh","local"]`.

`describe` returns machine-readable action input schemas. `docs` regenerates the current reference, including imported plugins. `run` reuses the selected session. `--confirm` is only for an action the user has explicitly approved; `--sudo` is opt-in and also needs confirmation. No operation bypasses installation, enablement, permission or input checks. Output is bounded and carries timing/truncation metadata. Local targets require a running GUI-owned local session.

## `crontab-list`

List the login user's crontab entries.

```sh
vibeshell plugins run cron-scheduler crontab-list --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "List the login user's crontab entries.",
  "elevate": false,
  "id": "crontab-list",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "My crontab",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `timers`

List all systemd timers with their next and last runs.

```sh
vibeshell plugins run cron-scheduler timers --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "List all systemd timers with their next and last runs.",
  "elevate": false,
  "id": "timers",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "systemd timers",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `cron-d`

Show every file in /etc/cron.d with its contents.

```sh
vibeshell plugins run cron-scheduler cron-d --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Show every file in /etc/cron.d with its contents.",
  "elevate": false,
  "id": "cron-d",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "cron.d drop-ins",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `crontab-file`

Read the system-wide crontab file.

```sh
vibeshell plugins run cron-scheduler crontab-file --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Read the system-wide crontab file.",
  "elevate": false,
  "id": "crontab-file",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "/etc/crontab",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `cron-add`

Append one entry to the login user's crontab.

```sh
vibeshell plugins run cron-scheduler cron-add --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Append one entry to the login user's crontab.",
  "elevate": false,
  "id": "cron-add",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "line": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "line"
    ],
    "type": "object"
  },
  "name": "Add cron entry",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `cron-remove`

Remove one exact entry from the login user's crontab.

```sh
vibeshell plugins run cron-scheduler cron-remove --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Remove one exact entry from the login user's crontab.",
  "elevate": false,
  "id": "cron-remove",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "line": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "line"
    ],
    "type": "object"
  },
  "name": "Remove cron entry",
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
