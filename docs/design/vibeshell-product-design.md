# VibeShell Product Design Document

> **Version:** 1.0
> **Date:** 2026-01-23
> **Status:** Finalized

---

## 1. Product Overview

### 1.1 Vision

VibeShell is a high-performance SSH/SFTP terminal built with Tauri, featuring:
- **Session Sharing** between GUI and CLI, allowing AI to attach and execute commands
- **MCP Native** integration for AI coding tools (Claude Code, Cursor, Codex, Open Code)
- **One-Click Installation** of SSH management capabilities to AI tools

### 1.2 Target Users

- **Primary:** Developers managing personal servers (1-50 servers)
- **Secondary:** DevOps teams (architecture supports future team features)

### 1.3 Core Value Proposition

| Traditional SSH Tools | VibeShell |
|----------------------|-----------|
| Disconnected sessions | Persistent sessions shareable between GUI/CLI/AI |
| Manual server management | AI-assisted management via MCP |
| Separate tools for each AI IDE | One-click integration to all major AI tools |

---

## 2. Architecture

### 2.1 System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      VibeShell                              │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐    ┌─────────────────────────────────┐ │
│  │   CLI (vshell)  │    │         Tauri GUI App           │ │
│  │  Independent    │    │  ┌─────────────────────────┐   │ │
│  │  Binary         │    │  │  React + TypeScript     │   │ │
│  └────────┬────────┘    │  │  └── xterm.js Terminal  │   │ │
│           │             │  │  └── SFTP File Manager  │   │ │
│           │             │  │  └── Server List UI     │   │ │
│           │             │  └───────────┬─────────────┘   │ │
│           │             └──────────────┼─────────────────┘ │
│           │                            │                   │
│  ┌────────▼────────────────────────────▼─────────────────┐ │
│  │              Rust Core (vibeshell_core)               │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │ │
│  │  │SSH Engine│ │SFTP Engine│ │ Session  │ │Credential│  │ │
│  │  │ (russh)  │ │ (russh)  │ │ Manager  │ │ Storage  │  │ │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘  │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────────────────────┐│ │
│  │  │ Server   │ │Recording │ │     MCP Server           ││ │
│  │  │ Storage  │ │ Engine   │ │  (All tools exposed)     ││ │
│  │  │(SQLite)  │ │          │ │                          ││ │
│  │  └──────────┘ └──────────┘ └──────────────────────────┘│ │
│  └───────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Session Sharing Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                   Session Manager (Rust)                    │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐    │
│  │              Session Pool (Memory + Persist)         │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐            │    │
│  │  │Session A │ │Session B │ │Session C │   ...      │    │
│  │  │ id: abc  │ │ id: def  │ │ id: ghi  │            │    │
│  │  │ server:  │ │ server:  │ │ server:  │            │    │
│  │  │ prod-1   │ │ dev-db   │ │ staging  │            │    │
│  │  └────┬─────┘ └────┬─────┘ └────┬─────┘            │    │
│  └───────┼────────────┼────────────┼──────────────────┘    │
│          │            │            │                        │
│  ┌───────▼────────────▼────────────▼──────────────────┐    │
│  │              Session Bus (Multi-client Broadcast)   │    │
│  │  • Output broadcast to all connected clients        │    │
│  │  • Input arbitration (who controls)                 │    │
│  │  • Client connect/disconnect doesn't affect session │    │
│  └───────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
         │                    │                    │
    ┌────▼────┐          ┌────▼────┐          ┌────▼────┐
    │  GUI    │          │  CLI    │          │   MCP   │
    │ Client  │          │ Client  │          │ Client  │
    │(Tauri)  │          │(vshell) │          │  (AI)   │
    └─────────┘          └─────────┘          └─────────┘
```

**Session Sharing Commands:**

| Command | Function |
|---------|----------|
| `vshell sessions` | List all active sessions |
| `vshell attach <session-id>` | Connect to existing session |
| `vshell detach` | Disconnect (session keeps running) |
| `vshell exec <session-id> "cmd"` | Execute in session without attach |
| `vshell watch <session-id>` | Read-only observe session output |
| `vshell kill <session-id>` | Terminate session |

**Input Control Modes:**
- **Exclusive:** One client controls, others read-only
- **Collaborative:** Multiple clients can input (use carefully)
- **Queue:** Inputs queued for execution (good for AI batch commands)

---

## 3. Data Model

### 3.1 Database Schema (SQLite)

```sql
-- Server configurations
CREATE TABLE servers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    host TEXT NOT NULL,
    port INTEGER NOT NULL DEFAULT 22,
    username TEXT NOT NULL,
    auth_type TEXT NOT NULL,  -- 'password' | 'key' | 'key_with_passphrase'
    credential_id TEXT,
    group_id TEXT,
    tags TEXT NOT NULL DEFAULT '[]',  -- JSON array
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Server groups (hierarchical)
CREATE TABLE groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    parent_id TEXT,
    color TEXT NOT NULL DEFAULT '#808080'
);

-- Encrypted credentials
CREATE TABLE credentials (
    id TEXT PRIMARY KEY,
    credential_type TEXT NOT NULL,  -- 'password' | 'private_key'
    encrypted_data BLOB NOT NULL,   -- AES-256-GCM encrypted
    created_at INTEGER NOT NULL
);

-- Session recordings
CREATE TABLE recordings (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    server_id TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    file_path TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'local'  -- 'local' | 'syncing' | 'synced'
);

-- Application settings
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

### 3.2 Encryption

- **Key Derivation:** Argon2 (master password → encryption key)
- **Encryption:** AES-256-GCM (authenticated encryption)
- **Master Password:** Never stored, only in memory during session

---

## 4. MCP Integration

### 4.1 MCP Tools

```typescript
// Server Management
server_list        // List all servers (filter by group/tags)
server_get         // Get server details
server_add         // Add new server
server_update      // Update server config
server_delete      // Delete server
group_list         // List groups
group_manage       // Create/update/delete groups

// Session Management
session_list       // List active sessions
session_create     // Create new session (connect to server)
session_attach     // Attach to existing session
session_detach     // Detach from session
session_kill       // Terminate session

// Command Execution
exec               // Execute command in session
exec_batch         // Batch execute (multi-server/multi-command)
output_read        // Read session output (supports streaming)
input_send         // Send input to session

// SFTP Operations
sftp_ls            // List remote directory
sftp_upload        // Upload file
sftp_download      // Download file
sftp_mkdir         // Create directory
sftp_rm            // Delete file/directory
sftp_move          // Move/rename

// Monitoring & Recording
status             // Connection status overview
recording_start    // Start recording
recording_stop     // Stop recording
recording_list     // List recordings
recording_play     // Playback recording
```

### 4.2 MCP Usage Example (AI Perspective)

```
User: "Help me check nginx status on production servers"

AI calls:
1. server_list(tags: ["prod"]) → finds prod-1, prod-2
2. session_create(server: "prod-1") → session_id: "abc123"
3. exec(session: "abc123", command: "systemctl status nginx")
4. output_read(session: "abc123") → gets output
5. session_kill(session: "abc123") → task done, terminate session
```

---

## 5. CLI Command System

```bash
# Global commands
vshell                          # Launch GUI (if installed)
vshell --help                   # Help
vshell --version                # Version

# Server management
vshell server list              # List all servers
vshell server list --group=prod # Filter by group
vshell server add               # Interactive add
vshell server add --name=prod-1 --host=1.2.3.4 --user=root
vshell server edit <name>       # Edit server
vshell server rm <name>         # Delete server

# Quick connect
vshell ssh <server-name>        # SSH connect (new session)
vshell sftp <server-name>       # Open SFTP session

# Session management
vshell sessions                 # List active sessions
vshell attach <session-id>      # Attach to session
vshell detach                   # Detach from current session
vshell kill <session-id>        # Kill session
vshell kill --all               # Kill all sessions

# Command execution (no attach needed)
vshell exec <server> "command"  # Execute single command
vshell exec --batch servers.txt "uptime"  # Batch execute

# SFTP shortcuts
vshell upload <server> <local> <remote>
vshell download <server> <remote> <local>
vshell ls <server>:<path>       # List remote directory

# Recording
vshell record start <session>   # Start recording
vshell record stop <session>    # Stop recording
vshell record list              # List recordings
vshell record play <id>         # Playback

# MCP Server
vshell mcp-server               # Start MCP Server
vshell mcp-server --port=3000   # Custom port

# AI Tool Integration
vshell install claude-code      # Install to Claude Code
vshell install cursor           # Install to Cursor
vshell install --all            # Install to all detected tools
vshell uninstall <tool>         # Uninstall
```

---

## 6. GUI Design

### 6.1 Main Layout

```
┌─────────────────────────────────────────────────────────────────────────┐
│  VibeShell                                        ─  □  ×              │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌─────────────────────────────────────────────────┐ │
│  │   Servers    │  │  Session Tabs                                   │ │
│  │              │  │  ┌─────────┐ ┌─────────┐ ┌─────────┐           │ │
│  │  ▼ Production│  │  │ prod-1  │ │ dev-db  │ │ + New   │           │ │
│  │    • prod-1  │  │  └────┬────┘ └─────────┘ └─────────┘           │ │
│  │    • prod-2  │  ├───────┴─────────────────────────────────────────┤ │
│  │              │  │                                                 │ │
│  │  ▼ Dev       │  │  root@prod-1:~# systemctl status nginx         │ │
│  │    • dev-db  │  │  ● nginx.service - A high performance web...   │ │
│  │    • dev-api │  │     Loaded: loaded (/lib/systemd/system/...    │ │
│  │              │  │     Active: active (running) since Mon...      │ │
│  │  ▼ Staging   │  │                                                 │ │
│  │    • staging │  │  root@prod-1:~# █                               │ │
│  │              │  │                                                 │ │
│  ├──────────────┤  ├─────────────────────────────────────────────────┤ │
│  │   Actions    │  │  SFTP Panel (expandable)                [↗]    │ │
│  │              │  │  /var/www/html/                                 │ │
│  │  + Add Server│  │  ├── index.html    2.3KB   Jan 20              │ │
│  │  ⚡ Quick Cmd │  │  ├── app.js        45KB    Jan 19              │ │
│  │  📁 SFTP     │  │  └── assets/       <DIR>   Jan 15              │ │
│  │  ⚙️ Settings │  │                                                 │ │
│  └──────────────┘  └─────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

### 6.2 SFTP Dual-Pane File Manager (Expanded)

```
┌─────────────────────────────────────────────────────────────────────────┐
│  SFTP File Manager - prod-1                               ─  □  ×      │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌────────────────────────────┐  ┌────────────────────────────┐        │
│  │  Local: C:\Users\Ricky     │  │  Remote: /var/www/html     │        │
│  ├────────────────────────────┤  ├────────────────────────────┤        │
│  │  📁 ..                     │  │  📁 ..                     │        │
│  │  📁 Documents              │  │  📁 assets                 │        │
│  │  📁 Downloads              │  │  📄 index.html      2.3KB  │        │
│  │  📄 deploy.sh       1.2KB  │  │  📄 app.js          45KB   │        │
│  │  📄 config.json     0.5KB  │  │  📄 style.css       12KB   │        │
│  └────────────────────────────┘  └────────────────────────────┘        │
│                                                                         │
│        [ ← Upload ]    [ Download → ]    [ Sync ⇄ ]    [ Delete ]      │
│                                                                         │
│  Transfer Queue:                                                        │
│  ├── ✅ deploy.sh → /var/www/html/deploy.sh (completed)                │
│  └── ⏳ assets.zip → /var/www/html/assets.zip (45%...)                 │
└─────────────────────────────────────────────────────────────────────────┘
```

### 6.3 Settings - AI Tool Integration

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Settings → Integrations                                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  🔌 AI Tool Integrations                                                │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │   │
│  │  │ Claude Code  │  │  Open Code   │  │    Codex     │          │   │
│  │  │  ✅ Installed │  │  ⬜ Install  │  │  ⬜ Install  │          │   │
│  │  └──────────────┘  └──────────────┘  └──────────────┘          │   │
│  │                                                                 │   │
│  │  ┌──────────────┐                                              │   │
│  │  │    Cursor    │                                              │   │
│  │  │  ⬜ Install  │                                              │   │
│  │  └──────────────┘                                              │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  📦 What gets installed:                                                │
│  • MCP Server config (points to VibeShell)                             │
│  • SSH management tools (server_list, exec, sftp_*, etc.)              │
│  • Session sharing capabilities                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 7. Technology Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| Framework | Tauri 2.x | Native performance, small bundle |
| Frontend | React 18 + TypeScript + Vite | Rich ecosystem, xterm.js integration |
| Terminal | xterm.js + addons | Mature, feature-rich |
| SSH/SFTP | russh | Pure Rust, no external deps |
| Database | SQLite (rusqlite) | Embedded, reliable |
| Encryption | ring + argon2 | Industry standard, audited |
| State Management | Zustand | Simple, performant |
| UI Components | Tailwind CSS + shadcn/ui | Modern, customizable |

---

## 8. Implementation Phases

### Phase 1: Foundation ✅
- Tauri project setup
- Rust workspace (CLI + GUI shared core)
- SQLite database
- Credential encryption
- Server CRUD

### Phase 2: SSH Connection
- russh SSH client
- Session manager
- xterm.js frontend
- Tauri commands

### Phase 3: Session Sharing
- IPC for CLI-GUI communication
- CLI session commands
- Multi-client session bus

### Phase 4: SFTP
- SFTP client
- File operations
- UI file manager

### Phase 5: MCP Integration
- MCP Server
- All tools implementation
- One-click AI tool installation

### Phase 6: Frontend UI
- Server list component
- Settings page
- Complete UI polish

---

## 9. Future Considerations (Not in v1.0)

- **Team Features:** User roles, audit logs, shared credentials
- **Cloud Sync:** Cross-device session and recording sync
- **Jump Hosts:** ProxyJump support for bastion hosts
- **2FA:** Hardware key support (YubiKey, etc.)
- **Plugins:** Extensible scripting and automation

---

## Appendix A: File Structure

```
vibeshell/
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── main.rs              # Tauri entry
│   │   ├── lib.rs               # Core library
│   │   ├── ssh/                 # SSH module
│   │   ├── sftp/                # SFTP module
│   │   ├── session/             # Session manager
│   │   ├── storage/             # Database + crypto
│   │   ├── mcp/                 # MCP Server
│   │   ├── ipc/                 # IPC for CLI-GUI
│   │   ├── commands/            # Tauri commands
│   │   └── install/             # AI tool installer
│   └── Cargo.toml
│
├── src/                          # React frontend
│   ├── components/
│   │   ├── Terminal/
│   │   ├── ServerList/
│   │   ├── FileManager/
│   │   ├── SessionTabs/
│   │   └── Settings/
│   ├── stores/
│   └── App.tsx
│
├── cli/                          # Standalone CLI
│   └── src/main.rs
│
├── docs/
│   ├── design/                  # Design documents
│   └── plans/                   # Implementation plans
│
└── package.json
```

---

*Document generated from brainstorming session on 2026-01-23*
