# CLAUDE.md — VibeShell

## What is this project?
VibeShell is a desktop SSH/SFTP terminal application. Think of it as a modern alternative to PuTTY/Tabby/Xshell, built with Tauri 2 (Rust) + React 18 (TypeScript).

## Quick start
```bash
npx tauri dev                    # Dev mode (hot reload)
npm run build && cargo check     # Verify both sides compile
unset CI CXX CC && npx tauri build --no-bundle  # Release exe
```

## Key files to know

| What | Where |
|------|-------|
| App entry + layout | `src/App.tsx` |
| All Tauri commands registered | `src-tauri/src/lib.rs` (invoke_handler) |
| Backend command implementations | `src-tauri/src/commands/*.rs` |
| SSH client (russh) | `src-tauri/src/ssh/client.rs` |
| Session manager | `src-tauri/src/session/manager.rs` |
| Database schema + CRUD | `src-tauri/src/storage/database.rs` |
| Data models | `src-tauri/src/storage/models.rs` |
| Frontend Tauri wrapper | `src/lib/tauri.ts` (`safeInvoke`) |
| Zustand stores | `src/stores/*.ts` |
| Tailwind theme config | `tailwind.config.js` |
| Tauri config | `src-tauri/tauri.conf.json` |

## Architecture in 30 seconds

**Frontend** (React + Zustand) → `safeInvoke('command', args)` → **Tauri IPC** → **Rust command handler** → business logic (SSH/SFTP/DB/tunnel)

Session I/O: `mpsc::channel` for input, `broadcast::channel` for output. Terminal uses xterm.js with WebGL renderer.

**Database:** SQLite via rusqlite. Tables: servers, groups, credentials, tunnel_configs, command_snippets, recordings, settings.

## Gotchas & constraints

### russh 0.44 (SSH library)
- `client::Handle` and `Channel` are **NOT Clone**
- For shared handle access: `Arc<tokio::sync::Mutex<Option<Handle>>>`
- For channel I/O: `channel.into_stream()` then `tokio::io::split()`
- Never try `.clone()` on Handle or Channel — it won't compile

### Theme system
- All colors use CSS custom properties via Tailwind: `bg-tokyo-bg`, `text-tokyo-fg`, `border-tokyo-bg-hl`
- Available theme colors: `tokyo-bg`, `tokyo-bg-dark`, `tokyo-bg-hl`, `tokyo-fg`, `tokyo-comment`, `tokyo-selection`, `tokyo-blue`, `tokyo-green`, `tokyo-red`, `tokyo-yellow`, `tokyo-magenta`, `tokyo-cyan`
- **Do not** use hardcoded hex colors like `#1a1b26` — they break theme switching

### safeInvoke pattern
```typescript
const result = await safeInvoke<MyType>('command_name', { arg1: 'value' });
if (result.success) {
  // result.data is MyType
} else {
  // result.error is TauriError (has .message, .isTauriUnavailable)
}
```

### Windows build
`unset CI CXX CC` before `npx tauri build` to avoid cc-rs detecting wrong MinGW toolchain.

## Adding a new feature (checklist)

### Backend (Rust)
1. Add model struct to `src-tauri/src/storage/models.rs`
2. Add DB table/migration to `database.rs::init_schema()` (use `ALTER TABLE ADD COLUMN` for existing tables, `CREATE TABLE IF NOT EXISTS` for new)
3. Add CRUD methods to `database.rs`
4. Create command file in `src-tauri/src/commands/your_feature.rs`
5. Export in `src-tauri/src/commands/mod.rs`
6. Register commands in `src-tauri/src/lib.rs` invoke_handler

### Frontend (React + TypeScript)
1. Add TypeScript types to `src/types/` or inline
2. Create Zustand store in `src/stores/yourStore.ts` using `safeInvoke`
3. Create component in `src/components/YourComponent/`
4. Use `tokyo-*` Tailwind classes, `cursor-pointer` on interactive elements, `transition-colors` on hover states
5. Wire into `App.tsx` if needed

### Verify
```bash
npm run build       # TS + Vite
cargo check         # Rust
cargo test          # Unit tests
```

## Testing
- Rust: `cargo test` (unit tests in modules, integration test in `tests/`)
- Frontend: No test framework yet
- Manual: `npx tauri dev` then interact with UI

## Tauri commands by module

**Session:** session_list, session_create, session_connect, session_kill, session_kill_all, session_send_input, session_send_bytes, session_resize, session_attach, session_detach, get_server_status

**Server:** get_servers, add_server, update_server, delete_server, get_groups, add_group, delete_group

**SFTP:** sftp_init, sftp_list_dir, sftp_download_file, sftp_upload_file, sftp_mkdir, sftp_delete, sftp_rename, sftp_pwd, sftp_stat, sftp_read_file, sftp_write_file, sftp_compress, sftp_extract

**Tunnel:** tunnel_config_list, tunnel_config_add, tunnel_config_update, tunnel_config_delete, tunnel_start, tunnel_stop, tunnel_list_active, tunnel_stop_all_for_session

**Snippet:** snippet_list, snippet_add, snippet_update, snippet_delete, snippet_search

**Recording:** start_recording, stop_recording, list_recordings, is_session_recording, get_session_recording_id, delete_recording, get_recording_content

**Local Shell:** local_shell_list_shells, local_shell_get_default, local_shell_list_sessions, local_shell_create, local_shell_send_input, local_shell_send_bytes, local_shell_resize, local_shell_attach, local_shell_detach, local_shell_kill, local_shell_kill_all

**Fingerprint:** get_fingerprint, save_fingerprint, delete_fingerprint, delete_fingerprint_by_id, list_fingerprints, verify_fingerprint, touch_fingerprint, clear_fingerprints

**Credential:** save_credential, get_credential, delete_credential

**Install:** detect_ai_tools, install_to_tool, uninstall_from_tool

**Dialog:** pick_ssh_key_file, pick_file_for_upload, pick_download_directory, read_ssh_key_file
