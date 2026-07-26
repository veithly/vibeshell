# SFTP Transfer Deliverable

Status: complete.

## Final Behavior

- File and directory deletion works across local sessions, direct SSH sessions, and remote IPC sessions.
- Remote deletion no longer requires a separate SFTP metadata request before REMOVE/RMDIR, improving compatibility with limited servers.
- Delete failures name the affected file and preserve the backend/server error instead of showing only a generic failure count.
- Upload accepts multiple files from one native picker action and processes them as one batch.
- Download processes every selected file into one chosen local directory.
- Upload and download show a persistent progress band with current item, processed/total progress, completed count, failed count, and final status.
- Multi-selection supports batch download, delete, compress, extract, copy paths, select all, and clear selection.
- Partial failures do not hide successful items.

## Boundary

Batch download currently handles selected files. Recursive remote directory download is not exposed because the backend has no remote-to-local directory transfer contract; directory-only selections keep Download disabled.

## Verification

- Focused SFTP interactions: 8 passed.
- Full frontend suite: 108 passed.
- Rust suite: 196 passed, 6 ignored.
- Frontend production build: passed.
- Rust check: passed.
- Installed bundle: arm64, version 1.0.2, identifier `com.vibeshell.desktop`, strict code-sign verification passed.
- Installed binary SHA-256 matched the built bundle: `142756662fe53dae51e3031366e1756abbcca07f5fe87f515a5659cf36f58f41`.
- Live installed-app QA loaded the local SFTP listing, opened a complete context menu, selected 127 entries, and exposed `Download 10 files`, `Compress (127 items)`, and `Delete (127 items)` without freezing.
- No real upload, download, or delete operation was performed during live QA.
