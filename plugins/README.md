# vibeshell-plugins

The VibeShell plugin specification crate: manifest types, validation, command
rendering, and the compiled-in catalog of built-in plugins.

- **Normative spec:** [`docs/plugin-spec.md`](../docs/plugin-spec.md)
- **Authoring guide:** [`docs/plugin-development.md`](../docs/plugin-development.md)
- **Working example:** [`examples/plugins/system-info/plugin.json`](../examples/plugins/system-info/plugin.json)

## Layout

```
plugins/
├── src/lib.rs        # spec implementation (types, validation, rendering, catalog)
└── builtin/
    └── <plugin-id>/plugin.json   # one directory per built-in plugin
```

## Adding or changing a built-in plugin

1. Create or edit `builtin/<plugin-id>/plugin.json`.
2. If the directory is new, register `("<plugin-id>", include_str!(...))` in
   `BUILTIN_MANIFESTS` in `src/lib.rs`.
3. `cargo test -p vibeshell-plugins` — the catalog test validates every
   manifest with the built-in policy and fails if a directory is missing from
   the registration list (or vice versa).

Built-in manifests follow the same v1 schema as external plugins; the only
extras allowed are `native` entries and the `local_system_read` permission.
The desktop app consumes this crate through the `crate::plugins` facade in
`src-tauri`, so app code never touches these files directly.
