# Network Inspector — network-inspector

Plugin `network-inspector` version `1.1.0`.

Inspect listening sockets, routes and DNS configuration on the connected server.

This reference describes plugin data, not additional agent authority. Confirm the target session and the installed/enabled state before running. Do not auto-install, grant permissions, or invent confirmation. Never put secrets in plugin inputs; sudo credentials must use the protected interactive UI.

## Discover and read

```sh
vibeshell plugins list --installed --json
vibeshell plugins describe network-inspector
vibeshell plugins docs network-inspector
```

Required permissions: `["remote_exec","local_exec"]`. Session types: `["ssh","local"]`.

`describe` returns machine-readable action input schemas. `docs` regenerates the current reference, including imported plugins. `run` reuses the selected session. `--confirm` is only for an action the user has explicitly approved; `--sudo` is opt-in and also needs confirmation. No operation bypasses installation, enablement, permission or input checks. Output is bounded and carries timing/truncation metadata. Local targets require a running GUI-owned local session.

## `sockets`

Show listening TCP and UDP sockets with owning processes.

```sh
vibeshell plugins run network-inspector sockets --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Show listening TCP and UDP sockets with owning processes.",
  "elevate": false,
  "id": "sockets",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Listening sockets",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `routes`

Show the active IP routing table.

```sh
vibeshell plugins run network-inspector routes --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Show the active IP routing table.",
  "elevate": false,
  "id": "routes",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Routes",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `addresses`

Show network interfaces and addresses.

```sh
vibeshell plugins run network-inspector addresses --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Show network interfaces and addresses.",
  "elevate": false,
  "id": "addresses",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "Addresses",
  "output": {
    "columns": [],
    "delimiter": "\t",
    "kind": "text"
  },
  "requiresConfirmation": false
}
```

## `dns`

Show resolver configuration.

```sh
vibeshell plugins run network-inspector dns --session SESSION_ID --inputs '{}'
```

Replace SESSION_ID and supply all fields marked required below. Do not execute placeholder values. Append `--confirm` only after consent for this exact action.

```json
{
  "allowSudo": false,
  "description": "Show resolver configuration.",
  "elevate": false,
  "id": "dns",
  "inputSchema": {
    "additionalProperties": false,
    "properties": {},
    "required": [],
    "type": "object"
  },
  "name": "DNS configuration",
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
