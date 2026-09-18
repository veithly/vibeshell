# Reproducing the README gallery

These are screenshots of the real React application, not separately drawn interface mockups. `main.ts` imports `src/main.tsx` after installing the official Tauri mock bridge. Only the data and native responses are synthetic.

## Isolation

Use the dedicated development origin on port **1421**. The fixture refuses a production build or another origin. Do not use a production Tauri window, copy its local storage, or import saved servers.

The six hosts are fictional documentation addresses in `192.0.2.0/24`. The project paths, account names, status readings, terminal output, agent transcript, Git diff and history entries are examples. No model or agent executable runs. AI prediction is disabled. Unknown native commands throw instead of forwarding to a real backend; example file writes affect only the in-memory map. The page has a visible synthetic-data label and a local-only connection policy.

The fixture uses the dedicated origin's local storage for workspace layout. Reloading resets only VibeShell demo keys on that origin. It does not touch the installed application's database, key material or other browser origins.

## Run

From the repository root, with its normal development dependencies installed:

```sh
node node_modules/typescript/bin/tsc -p scripts/readme-demo/tsconfig.json --noEmit
node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 1421 --strictPort
```

Open `http://127.0.0.1:1421/scripts/readme-demo/index.html?scene=collaboration` in a disposable browser context. Capture the page viewport at **1440 × 900**, device scale **1**, after fonts and content have loaded. English is the default; `&lang=zh` uses the existing Chinese application UI. The gallery's English UI is shared across README translations.

## Published scenes

| Screenshot | Starting scene | Actual UI interaction |
| --- | --- | --- |
| `tour-collaboration.png` | `collaboration` | Open **Agent command history**. The terminal, host status and durable-activity components render fixture responses. |
| `tour-agent-approval.png` | `collaboration` | Queue the demo request with `window.__VIBESHELL_DEMO__.approveDemo()`; wait for the real approval dialog. Do not present the request as executed. |
| `tour-agent-review.png` | `coding` | **More actions → Workspace changes**; the first changed file opens in the real diff view. |
| `tour-connections.png` | `coding` | **New Session → SSH → Icon view**. |
| `tour-agent-launcher.png` | `coding` | **New Session → Coding Agent**; enter a sample brief without pressing Start. |

The fixture also supports `scene=files` for the real Markdown file workspace and light theme. Its file view was inspected through DOM/accessibility output, but no file-workspace screenshot is included in this gallery. The demo-only plugin/status responses are not transport or service integration tests.

The controller exposes `replay()` to refresh the synthetic terminal output, `newSession()` to exercise the existing session synchronization, and `blocked`, `calls`, `errors` for inspection. A capture must not hide an error or substitute fake UI around a missing response. Successful captured scenes had no unhandled page errors or unimplemented native calls.

With ego-browser, reuse one task space for the whole gallery and finish it once. Its CLI reads stdin before running `-e`; when using a noninteractive command runner, append `< /dev/null` so the script receives EOF. Do not create repeated task spaces to diagnose a request waiting on stdin.

## What this demonstrates

These images show the presentation and user controls, not real SSH authentication, deployment, AI reasoning or a performance benchmark. GUI/CLI integration and native file/tunnel behavior require their own tests. The production entry never imports this fixture; no new runtime dependency was added.

Feature descriptions in the three READMEs were traced to the actual implementations, including older capabilities rather than just the latest release:

| Capability | Implementation |
| --- | --- |
| Command completion and optional AI prediction | `src/components/Terminal/useCompletion.ts`, `src/lib/aiCommandPrediction.ts` |
| Local agent launch modes and project selection | `src/components/CodingAgentLauncher/CodingAgentLauncher.tsx` |
| Git status and line-by-line review | `src/components/WorkspaceChangesPanel/WorkspaceChangesPanel.tsx` |
| Shared-session UI and operation history | `src/stores/sessionStore.ts`, `src/stores/agentActivityStore.ts`, `src/components/AgentActivityPanel/` |
| Human approvals | `src/components/AgentApprovalDialog/`, `src-tauri/src/mcp/guard.rs` |
| Quick command output | `src/components/QuickCommandDialog/` |
| Local/remote file tabs, Markdown and draft preservation | `src/components/FileWorkspace/`, `src/stores/fileWorkspaceStore.ts` |
| Custom CSS, preview and emergency recovery | `src/lib/customTheme.ts`, `src/components/Settings/CustomThemeEditor.tsx`, `docs/local-files-and-css-themes.md` |
| SFTP, synchronization integrity and excluded paths | `src/components/SftpPanel/`, `src-tauri/src/sftp/sync.rs` |
| Declarative plugins and agent-readable interfaces | `plugins/builtin/`, `src-tauri/src/plugins/agent.rs`, `cli/src/commands/plugins.rs` |
| Forwarding and session recording | `src-tauri/src/tunnel/`, `src-tauri/src/logging/` |
| Optional vault synchronization | `src/stores/cloudSyncStore.ts`, `src-tauri/src/cloud_sync/` |

Keep this distinction when updating the gallery: **real components, synthetic data, bounded claims**.
