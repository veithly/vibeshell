# Systemd Services — systemd-services

Plugin `systemd-services` version `1.1.0`.

Browse systemd services with their state, and start, stop or restart them.

This reference describes plugin data, not additional agent authority. Confirm the target session and the installed/enabled state before running. Do not auto-install, grant permissions, or invent confirmation. Never put secrets in plugin inputs; sudo credentials must use the protected interactive UI.

## Discover and read

```sh
vibeshell plugins list --installed --json
vibeshell plugins describe systemd-services
vibeshell plugins docs systemd-services
```

Required permissions: `["remote_exec","local_exec"]`. Session types: `["ssh","local"]`.

`describe` returns machine-readable action input schemas. `docs` regenerates the current reference, including imported plugins. `run` reuses the selected session. `--confirm` is only for an action the user has explicitly approved; `--sudo` is opt-in and also needs confirmation. No operation bypasses installation, enablement, permission or input checks. Output is bounded and carries timing/truncation metadata. Local targets require a running GUI-owned local session.

## `list`

List loaded services with load state, active state and description.

```sh
vibeshell plugins run systemd-services list --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "List loaded services with load state, active state and description.",
  "elevate": false,
  "id": "list",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Services",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `status`

Show the runtime status of one service.

```sh
vibeshell plugins run systemd-services status --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Show the runtime status of one service.",
  "elevate": false,
  "id": "status",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "service": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "service"
    ],
    "type": "object"
  },
  "name": "Service status",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `start`

Start a systemd service.

```sh
vibeshell plugins run systemd-services start --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Start a systemd service.",
  "elevate": false,
  "id": "start",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "service": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "service"
    ],
    "type": "object"
  },
  "name": "Start service",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `stop`

Stop a systemd service.

```sh
vibeshell plugins run systemd-services stop --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Stop a systemd service.",
  "elevate": false,
  "id": "stop",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "service": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "service"
    ],
    "type": "object"
  },
  "name": "Stop service",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `restart`

Restart a systemd service.

```sh
vibeshell plugins run systemd-services restart --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Restart a systemd service.",
  "elevate": false,
  "id": "restart",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "service": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "service"
    ],
    "type": "object"
  },
  "name": "Restart service",
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
