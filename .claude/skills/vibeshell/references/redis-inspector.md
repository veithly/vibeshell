# Redis Inspector — redis-inspector

Plugin `redis-inspector` version `1.2.0`.

Inspect and manage Redis: health, keyspace stats, key lookups, clients, slowlog and runtime configuration. Automatically uses a Docker redis container when redis-cli is not installed on the host.

This reference describes plugin data, not additional agent authority. Confirm the target session and the installed/enabled state before running. Do not auto-install, grant permissions, or invent confirmation. Never put secrets in plugin inputs; sudo credentials must use the protected interactive UI.

## Discover and read

```sh
vibeshell plugins list --installed --json
vibeshell plugins describe redis-inspector
vibeshell plugins docs redis-inspector
```

Required permissions: `["remote_exec","local_exec"]`. Session types: `["ssh","local"]`.

`describe` returns machine-readable action input schemas. `docs` regenerates the current reference, including imported plugins. `run` reuses the selected session. `--confirm` is only for an action the user has explicitly approved; `--sudo` is opt-in and also needs confirmation. No operation bypasses installation, enablement, permission or input checks. Output is bounded and carries timing/truncation metadata. Local targets require a running GUI-owned local session.

## `ping`

Check that the Redis server answers.

```sh
vibeshell plugins run redis-inspector ping --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Check that the Redis server answers.",
  "elevate": false,
  "id": "ping",
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
    "required": [],
    "type": "object"
  },
  "name": "Ping",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `info`

Read one INFO section from the Redis server.

```sh
vibeshell plugins run redis-inspector info --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Read one INFO section from the Redis server.",
  "elevate": false,
  "id": "info",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "section": {
        "description": "",
        "enum": [
          "all",
          "server",
          "clients",
          "memory",
          "persistence",
          "stats",
          "replication",
          "cpu",
          "keyspace"
        ],
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "section"
    ],
    "type": "object"
  },
  "name": "Server info",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `keyspace`

Show per-database key counts and expiry stats.

```sh
vibeshell plugins run redis-inspector keyspace --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Show per-database key counts and expiry stats.",
  "elevate": false,
  "id": "keyspace",
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
    "required": [],
    "type": "object"
  },
  "name": "Keyspace",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `db-size`

Return the number of keys in the selected database.

```sh
vibeshell plugins run redis-inspector db-size --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Return the number of keys in the selected database.",
  "elevate": false,
  "id": "db-size",
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
    "required": [],
    "type": "object"
  },
  "name": "Key count",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `scan-keys`

List keys matching a glob pattern.

```sh
vibeshell plugins run redis-inspector scan-keys --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "List keys matching a glob pattern.",
  "elevate": false,
  "id": "scan-keys",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "pattern": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "pattern"
    ],
    "type": "object"
  },
  "name": "Scan keys",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `get-key`

Read the string value stored under a key.

```sh
vibeshell plugins run redis-inspector get-key --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Read the string value stored under a key.",
  "elevate": false,
  "id": "get-key",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "key": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "key"
    ],
    "type": "object"
  },
  "name": "Get key value",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `key-ttl`

Show the remaining time-to-live of a key in seconds (-1 forever, -2 missing).

```sh
vibeshell plugins run redis-inspector key-ttl --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Show the remaining time-to-live of a key in seconds (-1 forever, -2 missing).",
  "elevate": false,
  "id": "key-ttl",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "key": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "key"
    ],
    "type": "object"
  },
  "name": "Key TTL",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `delete-key`

Delete one key from the database.

```sh
vibeshell plugins run redis-inspector delete-key --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Delete one key from the database.",
  "elevate": false,
  "id": "delete-key",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "key": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "key"
    ],
    "type": "object"
  },
  "name": "Delete key",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `clients`

List clients currently connected to the Redis server.

```sh
vibeshell plugins run redis-inspector clients --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "List clients currently connected to the Redis server.",
  "elevate": false,
  "id": "clients",
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
    "required": [],
    "type": "object"
  },
  "name": "Connected clients",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `slowlog`

Show the 20 slowest recent commands.

```sh
vibeshell plugins run redis-inspector slowlog --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Show the 20 slowest recent commands.",
  "elevate": false,
  "id": "slowlog",
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
    "required": [],
    "type": "object"
  },
  "name": "Slow log",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `bigkeys`

Sample the dataset and report the largest key per type.

```sh
vibeshell plugins run redis-inspector bigkeys --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Sample the dataset and report the largest key per type.",
  "elevate": false,
  "id": "bigkeys",
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
    "required": [],
    "type": "object"
  },
  "name": "Biggest keys",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `config-get`

Read one runtime configuration value.

```sh
vibeshell plugins run redis-inspector config-get --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Read one runtime configuration value.",
  "elevate": false,
  "id": "config-get",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "setting": {
        "description": "",
        "enum": [
          "maxmemory",
          "maxmemory-policy",
          "appendonly",
          "appendfsync",
          "save",
          "databases",
          "timeout",
          "tcp-keepalive",
          "lazyfree-lazy-eviction"
        ],
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "setting"
    ],
    "type": "object"
  },
  "name": "Runtime config",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `key-type`

Show the value type stored under a key.

```sh
vibeshell plugins run redis-inspector key-type --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": true,
  "description": "Show the value type stored under a key.",
  "elevate": false,
  "id": "key-type",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "key": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "key"
    ],
    "type": "object"
  },
  "name": "Key type",
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
