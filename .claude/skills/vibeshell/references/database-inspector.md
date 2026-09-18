# Database Inspector — database-inspector

Plugin `database-inspector` version `1.3.0`.

Read PostgreSQL, MySQL and SQLite metadata. When the client binary is missing on the host, PostgreSQL and MySQL are queried inside their Docker containers.

This reference describes plugin data, not additional agent authority. Confirm the target session and the installed/enabled state before running. Do not auto-install, grant permissions, or invent confirmation. Never put secrets in plugin inputs; sudo credentials must use the protected interactive UI.

## Discover and read

```sh
vibeshell plugins list --installed --json
vibeshell plugins describe database-inspector
vibeshell plugins docs database-inspector
```

Required permissions: `["remote_exec","local_exec"]`. Session types: `["ssh","local"]`.

`describe` returns machine-readable action input schemas. `docs` regenerates the current reference, including imported plugins. `run` reuses the selected session. `--confirm` is only for an action the user has explicitly approved; `--sudo` is opt-in and also needs confirmation. No operation bypasses installation, enablement, permission or input checks. Output is bounded and carries timing/truncation metadata. Local targets require a running GUI-owned local session.

## `postgres-databases`

List non-template PostgreSQL databases. Uses the host psql client, or a Docker postgres container when psql is absent.

```sh
vibeshell plugins run database-inspector postgres-databases --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "List non-template PostgreSQL databases. Uses the host psql client, or a Docker postgres container when psql is absent.",
  "elevate": false,
  "id": "postgres-databases",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "user": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [],
    "type": "object"
  },
  "name": "PostgreSQL databases",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `postgres-tables`

List user tables in a PostgreSQL database. Uses the host psql client, or a Docker postgres container when psql is absent.

```sh
vibeshell plugins run database-inspector postgres-tables --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "List user tables in a PostgreSQL database. Uses the host psql client, or a Docker postgres container when psql is absent.",
  "elevate": false,
  "id": "postgres-tables",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "database": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "user": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "database"
    ],
    "type": "object"
  },
  "name": "PostgreSQL tables",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `postgres-query`

Run one SQL statement using the remote psql authentication context, or inside a Docker postgres container.

```sh
vibeshell plugins run database-inspector postgres-query --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Run one SQL statement using the remote psql authentication context, or inside a Docker postgres container.",
  "elevate": false,
  "id": "postgres-query",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "database": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "query": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "user": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "database",
      "query"
    ],
    "type": "object"
  },
  "name": "PostgreSQL query",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `mysql-databases`

List databases visible to the configured MySQL client. Uses the host mysql client, or a Docker mysql/mariadb container when it is absent.

```sh
vibeshell plugins run database-inspector mysql-databases --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "List databases visible to the configured MySQL client. Uses the host mysql client, or a Docker mysql/mariadb container when it is absent.",
  "elevate": false,
  "id": "mysql-databases",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "user": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [],
    "type": "object"
  },
  "name": "MySQL databases",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `mysql-tables`

List tables in a MySQL database. Uses the host mysql client, or a Docker mysql/mariadb container when it is absent.

```sh
vibeshell plugins run database-inspector mysql-tables --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "List tables in a MySQL database. Uses the host mysql client, or a Docker mysql/mariadb container when it is absent.",
  "elevate": false,
  "id": "mysql-tables",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "database": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "user": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "database"
    ],
    "type": "object"
  },
  "name": "MySQL tables",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `mysql-query`

Run one SQL statement using the remote mysql authentication context, or inside a Docker mysql/mariadb container.

```sh
vibeshell plugins run database-inspector mysql-query --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Run one SQL statement using the remote mysql authentication context, or inside a Docker mysql/mariadb container.",
  "elevate": false,
  "id": "mysql-query",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "database": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "query": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "user": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "database",
      "query"
    ],
    "type": "object"
  },
  "name": "MySQL query",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `sqlite-files`

Find common SQLite database files below the remote login directory.

```sh
vibeshell plugins run database-inspector sqlite-files --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Find common SQLite database files below the remote login directory.",
  "elevate": false,
  "id": "sqlite-files",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "SQLite files",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `sqlite-query`

Open a SQLite file and run one SQL statement.

```sh
vibeshell plugins run database-inspector sqlite-query --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Open a SQLite file and run one SQL statement.",
  "elevate": false,
  "id": "sqlite-query",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "path": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "query": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "path",
      "query"
    ],
    "type": "object"
  },
  "name": "SQLite query",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `postgres-query-csv`

Run one SQL statement and return CSV rows with a header line.

```sh
vibeshell plugins run database-inspector postgres-query-csv --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Run one SQL statement and return CSV rows with a header line.",
  "elevate": false,
  "id": "postgres-query-csv",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "database": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "query": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "user": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "database",
      "query"
    ],
    "type": "object"
  },
  "name": "PostgreSQL query (CSV)",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `mysql-query-batch`

Run one SQL statement and return tab-separated rows with a header line.

```sh
vibeshell plugins run database-inspector mysql-query-batch --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Run one SQL statement and return tab-separated rows with a header line.",
  "elevate": false,
  "id": "mysql-query-batch",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "container": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "database": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "query": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "user": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "database",
      "query"
    ],
    "type": "object"
  },
  "name": "MySQL query (TSV)",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": true
}
```

## `sqlite-query-csv`

Open a SQLite file, run one SQL statement, and return CSV rows with a header line.

```sh
vibeshell plugins run database-inspector sqlite-query-csv --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Open a SQLite file, run one SQL statement, and return CSV rows with a header line.",
  "elevate": false,
  "id": "sqlite-query-csv",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {
      "path": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      },
      "query": {
        "description": "",
        "maxLength": 1024,
        "pattern": "^[^\\r\\n\\u0000]*$",
        "type": "string"
      }
    },
    "required": [
      "path",
      "query"
    ],
    "type": "object"
  },
  "name": "SQLite query (CSV)",
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
