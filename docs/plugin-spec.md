# VibeShell Plugin Specification — Version 1

Status: **Active** · Schema version: `1` · Implementation: [`plugins/`](../plugins) (`vibeshell-plugins` crate)

This document is the normative specification for VibeShell plugins. The
development guide ([plugin-development.md](plugin-development.md)) is the
companion how-to; when the two disagree, this document wins. The words MUST,
MUST NOT, SHOULD, and MAY are used as in RFC 2119.

## 1. Overview

A VibeShell plugin is a **declarative JSON manifest**. It describes actions —
a static program plus arguments — that VibeShell renders and executes against
a connected session. No third-party code (JavaScript, HTML, WASM, native
libraries) ever loads into the privileged app process. The host owns
validation, rendering, quoting, execution, and output capture.

Three roles interact with this spec:

| Role | Responsibility |
| --- | --- |
| **Author** | Writes a conforming manifest. |
| **Host** | A VibeShell build that validates, installs, renders, and executes plugins. |
| **Distributor** | Moves manifest files between devices (file share, repo, backup file). |

## 2. Distribution format

- A plugin is distributed as a **single JSON document**, UTF-8 encoded, at most
  256 KB (`MAX_MANIFEST_BYTES`).
- The document is the manifest described in §4. Export (§8) produces exactly
  this document; there is no wrapper, envelope, or signature in v1.
- The recommended file name is `<id>-<version>.plugin.json`. Hosts MUST accept
  any `.json` file whose contents parse as a manifest.
- v1 deliberately ships no remote registry, no package signing, and no update
  channel. Any future registry MUST add signature verification before it may
  distribute manifests.

## 3. Identity

Every plugin has an `id` that is globally stable across devices and backups:

- 3–64 characters; ASCII lowercase letters, digits, `.`, `_`, `-`; MUST start
  with a lowercase letter or digit (the same grammar as `category`, `icon`,
  action ids, and input ids).
- The host namespaces installed state by `id`. Two manifests with the same
  `id` are the *same plugin*; installing a newer version updates it (§7).
- External plugins MUST NOT use an `id` that collides with a built-in plugin;
  hosts MUST reject such imports.

## 4. Manifest data model

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
  "defaultSettings": {},
  "entry": {
    "type": "commands",
    "actions": [ /* §4.2 */ ]
  }
}
```

### 4.1 Top-level fields

| Field | Required | Rules |
| --- | --- | --- |
| `schemaVersion` | yes | MUST be `1`. |
| `id` | yes | §3 grammar. |
| `name` | yes | ≤ 80 bytes, non-empty. |
| `description` | yes | ≤ 600 bytes, non-empty. |
| `version` | yes | ≤ 32 bytes. Free-form; semver recommended. |
| `author` | yes | ≤ 80 bytes. |
| `category` | yes | Identifier grammar; used for marketplace grouping. |
| `icon` | yes | Identifier grammar; a Lucide icon name known to the host. Unknown names fall back to the plug icon. |
| `permissions` | yes | Non-empty list from §5.1, no duplicates. |
| `sessionTypes` | yes | Non-empty subset of `ssh`, `local`. |
| `defaultSettings` | no | JSON object; ≤ 16 KB serialized; MUST NOT contain secrets. |
| `entry` | yes | §4.2 (`commands`) or §4.4 (`native`). |

Hosts MUST ignore unrecognized fields so that forward-compatible additions
remain loadable by older builds. A host that rejects a `schemaVersion` it does
not support MUST report the highest version it supports.

### 4.2 Actions

An entry of type `commands` declares 1–24 actions:

```json
{
  "id": "container-logs",
  "name": "Container logs",
  "description": "Read recent logs.",
  "program": "docker",
  "args": ["logs", "--tail", "200", "{{input.container}}"],
  "inputs": [ /* §4.3 */ ],
  "requiresConfirmation": false,
  "elevate": false,
  "allowSudo": false,
  "output": { "kind": "text" }
}
```

| Field | Required | Rules |
| --- | --- | --- |
| `id` | yes | Identifier grammar; unique within the plugin. |
| `name` / `description` | yes | ≤ 80 / ≤ 300 bytes. |
| `program` | yes | ≤ 256 bytes; ASCII alphanumerics plus `_ - . / +`; no `..`. A binary name or absolute path. |
| `args` | no | ≤ 32 arguments, each ≤ 2048 bytes. |
| `inputs` | no | ≤ 8 inputs (§4.3). |
| `requiresConfirmation` | no | Default `false`. The host MUST prompt before every run when `true`. MUST be `true` when `elevate` is `true`. |
| `elevate` | no | Default `false`. Runs under sudo (built-in plugins only in practice; see §5.2). |
| `allowSudo` | no | Default `false`. Offers an explicit "try sudo" path without elevating by default. Mutually exclusive with `elevate`. |
| `output` | no | Default text (§4.5). |

**Templates.** An argument may be exactly `{{input.<id>}}` and nothing else.
Embedded templates (`prefix={{input.x}}`) MUST be rejected. Every referenced
input MUST be declared on the same action. Rendering substitutes the raw value
for the whole argument, then the host POSIX-shell-quotes the argument
(single-quote wrapping with `'"'"'` escaping; an empty value renders as `''`).
The frontend cannot submit a replacement command — it sends input *values*,
never commands.

The quoting guarantee protects the **outer command's argument structure**. It
is not a sandbox: the invoked program may itself interpret an argument as SQL,
a glob, or shell source. The manifest author owns that semantic boundary and
SHOULD set `requiresConfirmation` on actions that execute user-provided code
or mutate state.

### 4.3 Inputs

| Field | Required | Rules |
| --- | --- | --- |
| `id` | yes | Identifier grammar; unique within the action. |
| `label` | yes | ≤ 80 bytes. |
| `description` / `placeholder` | no | ≤ 240 / ≤ 120 bytes. |
| `required` | no | Default `false`. |
| `kind` | no | One of `text` (default), `integer`, `boolean`, `select`. |
| `options` | select only | 1–32 unique options, each ≤ 80 bytes. |

Values are limited to 1,024 bytes and MUST NOT contain NUL, LF, or CR. An
omitted optional input renders as an empty string argument (useful as `sh -c`
script positional arguments). Select inputs MUST submit one of the declared
options.

### 4.4 Native entries (built-in only)

`entry` may be `{ "type": "native", "view": "..." }`. Native views are
compiled React components shipped inside the app; the only defined view is
`server-status`. External manifests MUST NOT declare native entries — hosts
MUST reject them.

### 4.5 Output

```json
"output": { "kind": "table", "columns": ["ID", "Name"], "delimiter": "\t" }
```

- `kind`: `text` (default) or `table`.
- Tables declare 1–16 `columns` and a `delimiter` of 1–4 bytes (no newlines).
  The command SHOULD print one row per line; the host splits on the delimiter
  and pads/truncates cells to the column count, rendering at most 1,000 rows.
- Captured output is bounded to ~1 MB with UTF-8-boundary-preserving
  truncation (§6).

## 5. Permissions and security model

### 5.1 Permission set

| Permission | Grants | Who may declare it |
| --- | --- | --- |
| `remote_exec` | Run action commands on connected SSH sessions. | Anyone. Required for plugins supporting `ssh`. |
| `local_exec` | Run action commands in local shell sessions. | Anyone. Required for plugins supporting `local`. |
| `local_system_read` | Read host system metrics (native views). | **Built-in only.** |

### 5.2 Security invariants

1. Importing a manifest MUST NOT enable it. External plugins install disabled
   with an empty grant set; enabling requires explicit user confirmation and
   grants exactly the manifest's declared permissions.
2. Disabling a plugin MUST revoke its stored grants.
3. Built-in plugins are reviewed at build time and MAY auto-grant on install.
4. The host resolves the installed manifest and action **server-side** before
   executing anything; the renderer cannot synthesize commands.
5. Input values are normalized, size-checked, and quoted as single arguments
   (§4.2).
6. Elevated execution uses `sudo -S -p ''` with the password written to stdin
   (never on a command line), or `sudo -n` for NOPASSWD hosts when no
   password is supplied. Passwords live in memory only.
7. Manifests, settings, and inputs MUST NOT contain secrets. Database actions
   SHOULD rely on authentication already configured on the remote host
   (`.pgpass`, Unix sockets, `docker exec` into the service container, …).
8. Third-party UI code never runs in the app's webview.

`remote_exec` remains a powerful capability: the author picks the program and
fixed arguments. Users SHOULD only import manifests from sources they trust.

## 6. Execution semantics

The host renders `program` + `args` (with quoted inputs), then:

- **SSH sessions:** executes through the session's exec channel wrapped as
  `(cmd) 2>&1 | head -c 1000001`, merging stderr into the captured output and
  bounding it to `MAX_PLUGIN_OUTPUT_BYTES` (1 MB) with UTF-8-safe truncation.
- **Local sessions:** spawns `$SHELL -c <cmd>` with a 60-second timeout,
  merging stdout and stderr.
- The result carries `pluginId`, `actionId`, `output`, `durationMs`, and a
  `truncated` flag. Non-zero exit statuses surface through the merged output
  rather than a separate channel.

## 7. Lifecycle

| Transition | Semantics |
| --- | --- |
| **Import** | File picked by the user, size-checked (≤ 256 KB), parsed, validated with the *external* policy, stored. Result: installed, **disabled**, empty grants. |
| **Install (built-in)** | Copied from the compiled-in catalog. Result: installed, enabled, granted the manifest's permissions. |
| **Enable** | User confirms; grants ← manifest permissions. |
| **Disable** | Grants ← ∅. |
| **Update** | Importing a manifest whose `id` matches an installed plugin: non-sensitive settings are preserved, the manifest is replaced, the plugin is **disabled**, and grants are revoked. The user reviews and re-enables. |
| **Uninstall** | Installation record deleted. The built-in catalog entry (if any) remains visible as uninstalled. |
| **Settings** | Per-plugin JSON object, ≤ 16 KB, updated at runtime; included in backups (§9). |

## 8. Import / export

- **Import** takes any conforming manifest file (§2) and follows §7 Import.
- **Export** writes the manifest as pretty-printed JSON with a trailing
  newline, named `<id>-<version>.plugin.json`:
  - Installed external plugin → the manifest it was imported with.
  - Built-in plugin (installed or not) → the shipped manifest, doubling as an
    authoring template. Note that exporting a native-view built-in (e.g.
    `server-performance`) produces a document that cannot be re-imported as
    external (§4.4); it is useful for reference and templates.
- **Settings are never exported.** They are device state and travel only via
  backups (§9).

## 9. Backup and cloud sync

Plugin installations are a first-class sync entity (`plugin_installation`)
alongside servers, groups, and command snippets. Consequently every backup
and sync mechanism built on the sync pipeline — the encrypted portable backup
file and cloud sync providers — carries plugins automatically.

Payload (camelCase on the wire):

```json
{
  "pluginId": "example.remote-tools",
  "version": "1.0.0",
  "source": "external",
  "enabled": true,
  "manifestJson": "{ ...完整 manifest（仅 external）... }",
  "settingsJson": "{\"rows\":25}",
  "installedAt": 1730000000,
  "updatedAt": 1730000900
}
```

Rules:

1. External plugins carry their full `manifestJson`; built-in plugins set it
   to `null` because their manifest resolves from the local catalog.
2. **Granted permissions are never transported.** The receiving device
   recomputes them: enabled → exactly the manifest's declared permissions;
   disabled → none.
3. A restored plugin that is unknown on the receiving device (e.g. a built-in
   plugin from a newer app version, or an external manifest that no longer
   validates) restores **disabled with empty grants** rather than failing the
   backup import.
4. Deleting a plugin installation emits a sync tombstone like any other
   entity; portable backup files intentionally omit tombstones and merge
   rather than delete.

## 10. Built-in catalog (monorepo layout)

Built-in plugins live in the `plugins/` workspace crate, one directory per
plugin:

```
plugins/
├── Cargo.toml            # vibeshell-plugins
├── src/lib.rs            # spec implementation: types, validation, rendering, catalog
└── builtin/
    ├── server-performance/plugin.json
    ├── docker-containers/plugin.json
    ├── redis-inspector/plugin.json
    └── ...               # 10 built-ins today
```

Authoring rules for built-ins:

1. Add `plugins/builtin/<id>/plugin.json` **and** register the pair in
   `BUILTIN_MANIFESTS` (src/lib.rs). The directory-id ↔ registration-id test
   fails the build otherwise.
2. Built-ins are validated with the *builtin* policy: they may additionally
   declare `native` entries and `local_system_read`.
3. Every built-in manifest MUST satisfy the same v1 schema as external ones
   otherwise — built-ins get no schema exceptions.
4. The catalog is embedded at compile time (`include_str!`); shipping a new
   or updated built-in requires an app release.

## 11. Versioning and compatibility

- `schemaVersion` tracks the **manifest format**. v1 is the current version.
  A future v2 may add fields (older hosts ignore unknown fields, §4.1) or
  change semantics (requires a bump).
- Hosts MUST reject manifests whose `schemaVersion` they do not understand.
- The wire format of sync payloads (§9) is versioned separately by the sync
  envelope and is not part of manifest compatibility.
- Plugin `version` is opaque to the host; it is display and export naming
  only.
