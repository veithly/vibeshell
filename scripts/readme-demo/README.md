# Reproduce the README gallery

The fixture imports the current `src/main.tsx` and uses the real application components. It contains example Northstar servers and in-memory file/command responses. Unknown application commands fail; nothing is forwarded to an SSH server or model.

## Native macOS capture

From the repository root:

```bash
node node_modules/typescript/bin/tsc -p scripts/readme-demo/tsconfig.json --noEmit
VIBESHELL_README_CAPTURE=run VIBESHELL_SKIP_SKILL_AUTO_INSTALL=1 \
  npx tauri dev --no-watch --config scripts/readme-demo/native.capture.conf.json
```

This configuration uses the dedicated `com.vibeshell.readme-preview` identity and loopback port **1421**. The entry refuses any other native app identity. Do not use an installed application's identifier or import its saved data.

The capture-only Vite configuration aliases the frontend IPC entry to `fixture-core.ts`, leaving Tauri's native callbacks untouched. Application operations are handled by the in-memory fixture; native window and event operations remain real. `fixture-webview.ts` changes only the receiving URL for detached windows so they load the same safe fixture. Neither alias is used by the normal development or production build.

Change scenes with the fixed-name loopback driver:

```bash
curl -X POST 'http://127.0.0.1:1421/__readme/scene?name=panes'
curl 'http://127.0.0.1:1421/__readme/status'
```

Allowed scenes are `workspace`, `panes`, `panes-day`, `completion`, `sftp`, `extract`, `files`, `plugins`, `settings`, `coding` and `approval`. There is no arbitrary-script endpoint. The driver exists only with `VIBESHELL_README_CAPTURE=run` and the capture Vite configuration.

`native-scenes.ts` waits for actual UI content before reporting `stage: ready`. Completion uses the terminal's real input handler. SFTP extraction is shown as a menu action, not executed. The `panes` scene restores a four-way Mosaic layout of two terminals, a Markdown document and a plugin, and asserts that there is **exactly one native window**.

Capture only after `native: true`, `blocked: []` and `errors: []`. A hot-reload failure is not an acceptable final scene: reload the fixture cleanly, re-run, and keep the successful report. For the README glass images, use the isolated `VibeShell README Capture` virtual display (the source helper is `scripts/readme-demo/virtual-display.m`), show a macOS wallpaper in a separate window via `glass-backdrop.swift` (both helpers require the virtual display; the app stays hidden if it is missing), and capture only that display. If the window moved across displays but is not in the virtual display's active Space, an Accessibility **Raise** action on the isolated preview window can restore visibility. Never use the physical monitor for the final shot. The `compose-glass.py` script makes the two presentation boards from `day-virtual.png` and `night-virtual.png` in `test-results/readme-glass`. Keep the sample-data label visible, then close the preview, wallpaper and virtual display processes after capture.

## Browser inspection

For DOM inspection or README layout verification:

```bash
node node_modules/vite/bin/vite.js --host 127.0.0.1 --port 1421 --strictPort
```

Open `http://127.0.0.1:1421/scripts/readme-demo/index.html?scene=workspace`. Use `lang=zh` for Chinese and `theme=light` for light mode. Browser mode mocks native APIs and uses a solid background; it is not the source of the current native gallery.

The `window.__VIBESHELL_DEMO__` controller offers `ready()`, `replay()`, `newSession()`, `approveDemo()`, `plugin(id)`, `settings()`, `marketplace()` and `theme()`. Inspect `blocked`, `calls` and `errors` after interactions.

Only the dedicated fixture origin's workspace keys are reset. Detached windows keep the handoff storage. No real profile, saved credentials, home-directory files or production gateway data are copied. Approval requests and Agent transcripts are examples; no service restart or model call occurs.

Asset names, scene mappings and provenance are listed in [assets/readme](../../assets/readme/README.md). Keep the three README translations and this gallery in sync when visible product behavior changes.
