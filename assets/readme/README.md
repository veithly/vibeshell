# README image sources

The current gallery is captured from the latest local source in a real **macOS Tauri/WKWebView application** with the identifier `com.vibeshell.readme-preview`. It uses the production React components, native window material, window controls and detached-window handoff. No generated interface artwork is used.

The `scripts/readme-demo` fixture supplies fictional Northstar hosts, files, commands, metrics and Agent transcripts. Application IPC remains in memory; only native window/event operations reach the isolated host. The small “DEMO · example data” label stays visible. No live SSH server, model, personal directory listing or credential is used.

## Gallery

| Image | Scene | What to look for |
| --- | --- | --- |
| `workspace.png` | `workspace` | Latest icon-only toolbar, terminal, bottom performance dock and per-session history |
| `glass-showcase.png` | `panes` (night) | Night Glass macOS hero with four real Mosaic panes; only the space **outside** the window receives an editorial crop, wallpaper, and shadow |
| `panes.png` | `panes-day` | Matching Day Glass view: two SSH terminals, Markdown, and Docker in the **same native main window** |
| `workspace.png` | `workspace` | Icon-only toolbar, terminal, bottom performance dock and per-session history (secondary image) |
| `completion.png` | `completion` | Real terminal input and its command-completion popup, not a separately rendered suggestion list |
| `sftp.png` | `extract` | Remote config editor and the archive context menu containing **Extract here** |
| `files.png` | `files` | Day-theme Markdown source and preview |
| `plugins.png` | `plugins` | Current Docker dashboard, inventory filters and resource columns |
| `agent-review.png` | `coding` | Coding-agent presentation and Git review UI |
| `settings.png` | `settings` | Current appearance controls and theme presets |
| `approval.png` | `approval` | Current approval dialog, available for detailed documentation |

The native main window is 1200 × 800 logical pixels. The cover and Day Glass board contain screenshots taken on a temporary **1440 × 900 macOS virtual display**. The window uses AppKit material above a separate window showing Apple's Sonoma wallpaper; no wallpaper or gradient is drawn inside VibeShell. `compose-glass.py` crops and frames the real application pixels, adding a subtle shadow and editorial labels **outside** the application. No controls, terminal text, panes, blur, or material are redrawn. The temporary display and screenshot applications must be shut down after capturing, leaving the physical displays untouched.

`hero.svg` remains an editable static title using the existing VibeShell logo and theme palette. Body text, commands and links remain normal Markdown.

## Reproduce

See the [capture guide](../../scripts/readme-demo/README.md) for the isolated native configuration, scene driver and browser fallback. Before publishing a scene, check that its report has `native: true`, an empty `blocked` array and an empty `errors` array. Inspect text, menus and document content visually.

The gallery illustrates the development UI with sample data. Version availability and operational limits belong in the release notes and [feature reference](../../docs/FEATURES.md), not as repeated captions throughout the product README.
