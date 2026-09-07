# Window management

## Using tabs and splits

Drag a terminal, file or plugin tab to reorder its rail. Drag it to the left,
right, top or bottom edge of a page to split that page. A translucent preview
shows the destination. Existing panes are moved rather than duplicated. Drag
the splitter to adjust the ratio. The current workspace supports up to nine panes.

Drag a tab through the titlebar or a window edge to open a separate native
window. Its content is removed from the main window only after the receiving
window acknowledges the handoff. Creating a native window unsuccessfully must
leave the source tab intact. Terminal handoff reattaches to the existing session;
it does not run a new shell command or reconnect an SSH session.

On macOS and Windows, dragging the detached titlebar back over the main tab
strip returns the tab. Dropping over a main-window page edge creates a split.
Release of the actual primary mouse button ends the gesture; Escape cancels it.
A plain titlebar click does not start a drag. The return button is also available.
Closing a detached window returns its page, whereas closing a terminal tab asks
for confirmation before terminating that session.

Files use a recoverable editing buffer. Moving a file waits for an in-flight
save and temporarily locks the outgoing editor. A completed old save cannot
overwrite newer edits in a remounted pane. Moving a page never writes its draft
to the user's local or remote file.

## Saving and restoring

Layout changes are saved automatically. The save icon beside the tabs also
saves explicitly. The snapshot includes session/tab order, selected page,
main-window split tree and ratios, detached targets and native window bounds.
The runtime session IDs are remapped when shells are recreated after a restart.
Windows from disconnected displays are clamped into a currently connected display.

Restoring a layout is not restoring terminated programs. Local shells may be
recreated; unavailable SSH connections need reconnection. Finished coding agents
are not automatically rerun. Layout snapshots do not contain authentication
credentials or plugin commands. Dirty editor buffers are separate local recovery
data, not a remote file save.

A failed optional bounds capture keeps the last known geometry instead of
blocking application exit indefinitely. A layout-storage failure is reported;
the user can cancel exit or keep the previous saved layout. A session used by a
detached page must be returned first before its connection is terminated.

## Regression checks

Run `npm run test` and `npm run build`, then
`cargo check --manifest-path src-tauri/Cargo.toml`.

`VIBESHELL_UI_SMOKE=run` opts a Vite development run into the fixed native WebView
smoke scenario in `scripts/native-ui-bridge.ts`. It has no control endpoint and
is disabled in normal development and production builds. Use only a disposable,
single-session local workspace: it creates a test shell, drives real React
buttons and DOM drag events, opens actual Tauri windows, returns them through
actual inter-window messages, saves two panes, and closes the application.
Relaunching in the same test mode checks layout restoration and closes the test
shell. It never sends terminal commands.

Verified on the local macOS development build: new/close dialog, new local
shell, close/cancel confirmation, left and top pane moves, native tear-out,
return button, native-message bottom docking, save, application close, fresh
application startup, changed session IDs with the same two-pane layout.
Synthetic DOM events are not an OS-level physical mouse test. Mixed-DPI physical
multi-monitor dragging and Windows/Linux desktop runs require separate manual QA.
On platforms without a native primary-button state implementation, native
window dragging and the return button remain available; automatic cross-window
mouse-release docking is currently implemented for macOS and Windows.
