# Docker Containers — docker-containers

Plugin `docker-containers` version `1.4.0`.

Inspect and manage containers, images, live resource usage and recent container logs through the remote Docker CLI.

This reference describes plugin data, not additional agent authority. Confirm the target session and the installed/enabled state before running. Do not auto-install, grant permissions, or invent confirmation. Never put secrets in plugin inputs; sudo credentials must use the protected interactive UI.

## Discover and read

```sh
vibeshell plugins list --installed --json
vibeshell plugins describe docker-containers
vibeshell plugins docs docker-containers
```

Required permissions: `["remote_exec","local_exec"]`. Session types: `["ssh","local"]`.

`describe` returns machine-readable action input schemas. `docs` regenerates the current reference, including imported plugins. `run` reuses the selected session. `--confirm` is only for an action the user has explicitly approved; `--sudo` is opt-in and also needs confirmation. No operation bypasses installation, enablement, permission or input checks. Output is bounded and carries timing/truncation metadata. Local targets require a running GUI-owned local session.

## `containers`

List running and stopped containers.

```sh
vibeshell plugins run docker-containers containers --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "List running and stopped containers.",
  "elevate": false,
  "id": "containers",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Containers",
  "output": {
    "columns": [
      "ID",
      "Name",
      "Image",
      "Status",
      "Ports"
    ],
    "delimiter": "\t",
    "kind": "table"
  },
  "requiresConfirmation": false
}
```

## `stats`

Capture one resource-usage snapshot for running containers.

```sh
vibeshell plugins run docker-containers stats --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Capture one resource-usage snapshot for running containers.",
  "elevate": false,
  "id": "stats",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Live snapshot",
  "output": {
    "columns": [
      "Container",
      "CPU",
      "Memory",
      "Network",
      "Block I/O"
    ],
    "delimiter": "\t",
    "kind": "table"
  },
  "requiresConfirmation": false
}
```

## `images`

List locally available Docker images.

```sh
vibeshell plugins run docker-containers images --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "List locally available Docker images.",
  "elevate": false,
  "id": "images",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Images",
  "output": {
    "columns": [
      "Image",
      "ID",
      "Size",
      "Created"
    ],
    "delimiter": "\t",
    "kind": "table"
  },
  "requiresConfirmation": false
}
```

## `logs`

Read the latest 200 log lines from a container.

```sh
vibeshell plugins run docker-containers logs --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Read the latest 200 log lines from a container.",
  "elevate": false,
  "id": "logs",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "container"
    ],
    "type": "object"
  },
  "name": "Container logs",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `inspect`

Return Docker's structured container metadata.

```sh
vibeshell plugins run docker-containers inspect --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Return Docker's structured container metadata.",
  "elevate": false,
  "id": "inspect",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "container"
    ],
    "type": "object"
  },
  "name": "Inspect container",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `exec-command`

Run one non-interactive shell command inside a container.

```sh
vibeshell plugins run docker-containers exec-command --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Run one non-interactive shell command inside a container.",
  "elevate": false,
  "id": "exec-command",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "command": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "container",
      "command"
    ],
    "type": "object"
  },
  "name": "Run in container",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `start-container`

Start a stopped container.

```sh
vibeshell plugins run docker-containers start-container --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Start a stopped container.",
  "elevate": false,
  "id": "start-container",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "container"
    ],
    "type": "object"
  },
  "name": "Start container",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `stop-container`

Stop a running container gracefully.

```sh
vibeshell plugins run docker-containers stop-container --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Stop a running container gracefully.",
  "elevate": false,
  "id": "stop-container",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "container"
    ],
    "type": "object"
  },
  "name": "Stop container",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `restart-container`

Restart a container.

```sh
vibeshell plugins run docker-containers restart-container --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Restart a container.",
  "elevate": false,
  "id": "restart-container",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "container"
    ],
    "type": "object"
  },
  "name": "Restart container",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `volumes`

List Docker volumes and their drivers.

```sh
vibeshell plugins run docker-containers volumes --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "List Docker volumes and their drivers.",
  "elevate": false,
  "id": "volumes",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Volumes",
  "output": {
    "columns": [
      "Volume",
      "Driver",
      "Scope"
    ],
    "delimiter": "\t",
    "kind": "table"
  },
  "requiresConfirmation": false
}
```

## `networks`

List Docker networks.

```sh
vibeshell plugins run docker-containers networks --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "List Docker networks.",
  "elevate": false,
  "id": "networks",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Networks",
  "output": {
    "columns": [
      "Network",
      "Driver",
      "Scope"
    ],
    "delimiter": "\t",
    "kind": "table"
  },
  "requiresConfirmation": false
}
```

## `version`

Show the Docker server version.

```sh
vibeshell plugins run docker-containers version --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Show the Docker server version.",
  "elevate": false,
  "id": "version",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Docker version",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## MCP equivalent

Use `plugin_list` for installed state, `plugin_describe` with `plugin_id` and optional `reference: true` for this reference, and `plugin_execute` with `pluginId`, `actionId`, `sessionId`, and `inputs`. Gateway execution obtains required approval from the human; a model-supplied confirmation flag is not approval. SSH trust checks remain enabled.
