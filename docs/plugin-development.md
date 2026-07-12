# VibeShell Plugin Development

VibeShell plugin API v1 adds session tools without loading third-party code into the main webview. A plugin is a validated JSON manifest. VibeShell renders its controls and chooses the declared command on the Rust side.

## Marketplace Model

The plugin marketplace combines two sources:

- `builtin`: reviewed manifests shipped with VibeShell. Installing one enables it immediately.
- `external`: a local JSON manifest imported by the user. External plugins are unsigned, installed disabled, and require an explicit permission confirmation before they can run.

Open **Plugin Marketplace** from the workspace toolbar. Use **Import manifest** to select a local `plugin.json`. The complete working example is [examples/plugins/system-info/plugin.json](../examples/plugins/system-info/plugin.json).

To update an external plugin, import a newer manifest with the same `id`. VibeShell preserves its non-sensitive settings, replaces the manifest, disables the plugin, and revokes its grants. Review the new version before enabling it again.

Version 1 intentionally does not download remote packages or load JavaScript, HTML, Rust dynamic libraries, or WASM. A future remote registry needs package signing and update verification before it can safely extend this model.

## Manifest

```json
{
  "schemaVersion": 1,
  "id": "acme.service-tools",
  "name": "Service Tools",
  "description": "Inspect an Acme service on the connected server.",
  "version": "1.0.0",
  "author": "Acme",
  "category": "operations",
  "icon": "wrench",
  "permissions": ["remote_exec"],
  "sessionTypes": ["ssh"],
  "entry": {
    "type": "commands",
    "actions": [
      {
        "id": "service-status",
        "name": "Service status",
        "description": "Read the selected systemd unit status.",
        "program": "systemctl",
        "args": ["status", "{{input.service}}", "--no-pager"],
        "inputs": [
          {
            "id": "service",
            "label": "Service",
            "placeholder": "api.service",
            "required": true
          }
        ],
        "requiresConfirmation": true
      }
    ]
  }
}
```

### Top-level fields

| Field | Required | Rules |
| --- | --- | --- |
| `schemaVersion` | yes | Must be `1`. |
| `id` | yes | 3-64 lowercase letters, digits, `.`, `_`, or `-`; must start with a letter or digit. |
| `name` | yes | Human-readable name, at most 80 bytes. |
| `description` | yes | At most 600 bytes. |
| `version` | yes | Plugin version string, at most 32 bytes. |
| `author` | yes | At most 80 bytes. |
| `category` | yes | Uses the same identifier rules as `id`. |
| `icon` | yes | A host-known Lucide icon name. Unknown names use the plug icon. |
| `permissions` | yes | Command plugins must include `remote_exec`. |
| `sessionTypes` | yes | One or both of `ssh` and `local`. External command plugins currently execute only against SSH sessions. |
| `defaultSettings` | no | Non-secret JSON object, limited to 16 KB once installed. |
| `entry` | yes | `commands` for external plugins. `native` is reserved for compiled VibeShell plugins. |

## Actions

An external plugin can declare 1-24 actions. Each action has a static `program` and at most 32 arguments:

```json
{
  "id": "container-logs",
  "name": "Container logs",
  "description": "Read recent logs.",
  "program": "docker",
  "args": ["logs", "--tail", "200", "{{input.container}}"],
  "inputs": [
    {
      "id": "container",
      "label": "Container",
      "required": true,
      "kind": "text"
    }
  ]
}
```

Supported input kinds are `text`, `integer`, `boolean`, and `select`. An action may declare at most 8 inputs. A select input may declare up to 32 options. Labels, descriptions, placeholders, and options also have bounded lengths so a manifest cannot create an unbounded control tree. Input values are limited to 1,024 bytes and cannot contain NUL or newline characters.

Templates use the exact form `{{input.<id>}}`. Every referenced input must be declared. VibeShell substitutes the raw value into one argument and then POSIX-shell-quotes the complete argument. The frontend cannot submit a replacement command.

An input template must occupy a complete argument; forms such as `prefix={{input.value}}` are rejected. This protects the outer remote command's argument structure. It is not a sandbox for the invoked program: a program may intentionally interpret an argument as SQL, a regular expression, or shell source. The manifest author owns that semantic boundary.

Set `requiresConfirmation` on actions that execute user-provided code, modify state, or otherwise deserve a final review. VibeShell asks for confirmation every time the action runs.

Do not put passwords, tokens, private keys, SQL credentials, or other secrets in a manifest, plugin setting, or action input. Database actions should use authentication already configured on the remote server, such as Unix sockets, `.pgpass`, or an existing client profile.

## Output

Text is the default output:

```json
"output": { "kind": "text" }
```

For delimiter-separated rows, ask the host to render a table:

```json
"output": {
  "kind": "table",
  "columns": ["ID", "Name", "Status"],
  "delimiter": "\t"
}
```

The command should print one row per line. Table output supports 1-16 columns. VibeShell caps remote output at approximately 1 MB before it reaches the client and preserves UTF-8 boundaries when truncating it.

## Security Boundary

- External manifests cannot request a native React view or local system access.
- Importing a manifest does not enable it.
- Disabling a plugin revokes its stored permissions.
- The backend resolves the installed plugin and action before executing anything.
- User inputs are validated and quoted as individual arguments.
- Input templates cannot alter the outer argument structure, and remote output is bounded.
- Third-party UI code never runs in the privileged Tauri webview.

`remote_exec` is still a powerful permission: a manifest author chooses the program and fixed arguments. Review local manifests before enabling them and only import plugins from sources you trust.

## Built-in Catalog

VibeShell currently ships these installable plugins, all uninstalled by default:

- Server Performance
- Docker Containers
- Kubernetes Pods
- Database Inspector for PostgreSQL, MySQL, and SQLite metadata
- Process Explorer
- System Logs
- Network Inspector
- Disk Usage
- Git Workspace

Server Performance is the only native view. It replaces the former always-visible server status panel and owns its refresh and expansion settings after installation.
