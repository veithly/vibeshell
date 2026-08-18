---
name: vibeshell
description: Use the native VibeShell CLI to manage saved SSH servers, persistent sessions, remote commands, SFTP transfers, remote files, and SSH configuration imports without requiring the desktop UI.
---

# VibeShell

Use the native `vibeshell` executable. Do not use Node.js, `gateway.mjs`, browser automation, or raw credential extraction.

VibeShell automatically starts its local headless daemon when an SSH, SFTP, or session command needs it. The desktop application may be closed, and the same commands work on a server without a graphical desktop.

## Before operating

1. Verify the CLI is available with `vibeshell version`.
2. Inspect configured targets with `vibeshell servers`.
3. Prefer saved server names over raw hosts. Credentials remain local to VibeShell and must never be printed, copied into prompts, or passed on the command line.
4. Reuse an existing session unless the user explicitly needs an independent parallel shell.

Resolve the native executable in this order: `vibeshell` from `PATH`, `$HOME/.local/bin/vibeshell`, then `/Applications/VibeShell.app/Contents/MacOS/vibeshell` on macOS. Use the resolved absolute path for the rest of the workflow when necessary. If none exists, tell the user which lookup failed. Do not silently replace VibeShell with `ssh`, `scp`, or another client because that bypasses the saved VibeShell configuration and session model.

## Import existing SSH configurations

Preview everything VibeShell can discover:

```bash
vibeshell import auto --dry-run
```

Import detected OpenSSH, PuTTY, and Tabby profiles:

```bash
vibeshell import auto
```

Import one source or an explicit file:

```bash
vibeshell import openssh
vibeshell import openssh --path ~/.ssh/config
vibeshell import tabby --path ~/.config/tabby/config.yaml
vibeshell import putty --path ~/putty-sessions.reg
```

Use `--json` when structured output is more useful. Never import or expose plaintext passwords from third-party profiles. VibeShell may reference an existing private-key path and reads that local key only when establishing a connection.

## SSH sessions and remote commands

Open or reuse a persistent terminal session:

```bash
vibeshell ssh <server>
```

Run a command on a saved server. This reuses the earliest active session for that server when possible:

```bash
vibeshell ssh <server> -- uname -a
```

Create a separate session only when concurrency is intentional:

```bash
vibeshell ssh <server> --new
```

List sessions and use the short alias shown by VibeShell:

```bash
vibeshell sessions
vibeshell ssh-session 001 -- systemctl status nginx
vibeshell attach 001
vibeshell kill 001
```

For commands with nested quotes, multiline scripts, shell substitutions, or platform-sensitive escaping, avoid building a deeply escaped one-liner. Write the exact remote command to a local file or pipe it through stdin:

```bash
vibeshell ssh <server> --command-file ./remote-command.sh
cat ./remote-command.sh | vibeshell ssh <server> --command-stdin
```

If a remote process asks for interactive input, use named key tokens rather than starting another connection:

```bash
vibeshell send-key 001 yes enter
vibeshell send-key 001 ctrl-c
```

## Remote search and files

Search remotely with ripgrep-style output:

```bash
vibeshell rg <server> TODO /srv/app --glob '*.rs'
vibeshell rg --session 001 'listen 80' /etc/nginx -i
```

Read, create, and edit text files through SFTP:

```bash
vibeshell get-content <server> /etc/nginx/nginx.conf
vibeshell add-file <server> /tmp/example.txt --content-file ./example.txt --parents
vibeshell edit-file <server> /etc/app.conf --replace 'debug=false' --with 'debug=true'
```

Use a session alias with `--session` when a connection already exists:

```bash
vibeshell get-content --session 001 /var/log/app.log --max-bytes 200000
```

## SFTP

Run direct operations:

```bash
vibeshell sftp <server> ls /var/www
vibeshell sftp <server> get /remote/file ./local-file
vibeshell sftp <server> put ./local-file /remote/file
vibeshell sftp <server> sync ./dist /var/www --delete
```

Start the interactive SFTP prompt when several related operations are needed:

```bash
vibeshell sftp <server>
```

## Operational rules

- Inspect before mutating. Read the current file or directory before editing, deleting, overwriting, or synchronizing with `--delete`.
- Preserve session continuity. Prefer `ssh-session`, `--session`, and direct SFTP operations over repeatedly opening new SSH sessions.
- Treat `--new`, remote deletion, overwrites, recursive uploads, and sync deletion as deliberate actions.
- Keep secrets local. Do not request private-key text or passwords when an imported/saved profile should provide authentication.
- Report the server name, session alias, command exit status, and relevant output. Do not claim success from command submission alone.
- Use `vibeshell daemon status` for diagnostics. The daemon is native and does not require the desktop application.
