# Cloud Sync and Mobile Support

Status: encrypted manual sync and mobile runtime foundation implemented

## Outcomes

VibeShell should let a user move between desktop and mobile without rebuilding
their remote workspace by hand. The mobile app remains a local SSH/SFTP client;
the cloud sync plane stores only encrypted workspace data and does not proxy live
terminal traffic.

The first mobile release supports foreground SSH, terminal input, host-key
verification, server selection, and remote SFTP browsing. It does not promise
that an SSH socket stays alive while iOS or Android suspends the app.

## Product Boundaries

- Cloud storage never receives plaintext workspace records.
- Version 1 syncs server configurations, groups, and command snippets. Portable
  preference sync remains a later protocol extension.
- Credentials stay on the device until a Keychain/Keystore-backed vault and a
  separate cross-device encryption envelope are implemented.
- Trusted host fingerprints remain device-local so a compromised device cannot
  silently approve a host for every other device.
- Recordings remain device-local in version 1. They contain sensitive terminal
  output and currently reference absolute local file paths.
- Active sessions, local shells, open tabs, SFTP handles, live tunnels, Agent
  Gateway state, updater state, and local file paths are never sync records.
- Tunnel definitions are postponed until portable remote fields are separated
  from device-specific local ports, enablement, and auto-start behavior.

## Data Classification

| Data | Version 1 | Reason |
| --- | --- | --- |
| Servers | Encrypted sync | Core cross-device inventory; omit credential references |
| Groups | Encrypted sync | Portable organization metadata |
| Command snippets | Encrypted sync | Portable, but can contain secrets |
| Appearance and terminal preferences | Deferred | Portable subset needs an explicit schema |
| Credentials and private keys | Device-local | Runtime storage is currently plaintext and needs replacement |
| Host fingerprints | Device-local | Trust decision must be verified per device |
| Tunnel configurations | Deferred | Local bind ports and auto-start are device-specific |
| Recordings | Deferred, opt-in | Sensitive blobs with local paths and retention concerns |
| Sessions and local shells | Device-local runtime | Ephemeral resources, not durable records |

## Module Shape

The external seam stays deliberately small:

```rust
pub trait CloudSync {
    async fn sync(&self, trigger: SyncTrigger) -> Result<SyncReport>;
    fn status(&self) -> SyncStatus;
}
```

Callers do not manage cursors, encryption, batching, retries, conflict
resolution, or database transactions. Those behaviors remain inside the
module.

The only internal transport seam is an encrypted exchange:

```rust
pub trait SyncTransport {
    async fn exchange(&self, request: EncryptedBatch) -> Result<RemoteBatch>;
}
```

The implemented adapters target a private GitHub Gist or a WebDAV JSON object.
Both providers use conditional writes and store append-only encrypted envelopes;
an in-memory adapter covers two-device client tests. Transport code never
receives decoded server or snippet fields.

Desktop users can also export and import a versioned portable JSON snapshot.
This is an explicit local-file workflow rather than a background provider. The
file contains portable workspace records in plaintext, so the UI warns that it
may include sensitive hostnames and commands; credentials, fingerprints, and
recordings remain excluded.

```text
React settings/status
        |
        v
     CloudSync
        |
        +-- SQLite change journal, tombstones, cursor
        +-- deterministic merge and atomic apply
        +-- vault encryption and session-only unlock
        +-- persisted retry on lifecycle triggers and status reporting
        |
        v
  SyncTransport (ciphertext only)
```

## Local Change Model

Every syncable write appends an outbox row in the same SQLite transaction as
the domain write. Deletes create tombstones instead of disappearing from the
sync history. Existing installations bootstrap one current record for every
server, group, and command snippet.

Each record has:

- a stable entity ID;
- an entity kind;
- a schema version;
- a logical revision containing a counter and device ID;
- an optional deleted timestamp;
- a canonical JSON payload for non-deleted records.

Remote apply and cursor advancement happen in one transaction. Replaying a
remote batch is idempotent. A deterministic revision comparison resolves
concurrent edits; equal revisions with different payload hashes are reported as
integrity errors instead of silently picking one.

## Encryption Model

- Generate a random 256-bit vault key on the first paired device.
- The current release keeps the vault key only in process memory and requires
  the pairing code again after restart. Persist it only after a reviewed
  Keychain/Keystore adapter exists.
- Encrypt each batch with an authenticated cipher and a unique random nonce.
- Bind the wire version, algorithm, vault ID, batch ID, schema version, device
  ID, and provider cursor as authenticated associated data. Record revisions stay
  inside the AES-GCM-authenticated ciphertext rather than being duplicated in
  the routing metadata.
- Pair another device with the generated pairing code, which contains the
  provider configuration, vault ID, and vault key. QR and expiry remain future
  hardening. GitHub and WebDAV credentials remain independent of the vault key.
- Key rotation requires a future encrypted epoch/key-ID extension; version 1
  does not persist a vault key or rotate it in place.

The remote Gist or WebDAV object stores ciphertext and opaque cursor metadata.
It must not contain server hostnames, usernames, commands, tags, or group names
in plaintext.

## Mobile Runtime

The frontend reads backend runtime capabilities rather than inferring behavior
from the browser user agent.

| Capability | Desktop | iOS/Android version 1 |
| --- | --- | --- |
| SSH terminal | Yes | Yes, foreground |
| Remote SFTP browse/preview | Yes | Yes |
| File upload/download | Native paths | Deferred until platform picker adapters exist |
| Local shell | Yes | No |
| CLI IPC and Agent Gateway | Yes | No |
| Desktop self-update | Yes | No; use the app store |
| Split terminal panes | Yes | No on phone; optional on tablet later |
| Tunnels | Yes | Foreground-only, deferred from first release |
| Directory sync | Yes | Deferred until document-provider adapters exist |

Desktop-only Rust dependencies and startup behavior must be target-gated. The
mobile adapter obtains storage paths from `AppHandle::path()`. Credential
persistence is hidden in the mobile UI and rejected by the backend until native
Keychain/Keystore support exists. Native document pickers and explicit session
resume/suspend integration remain follow-up work.

## Delivery Phases

### Phase 0: Security and persistence

- [Pending] Replace plaintext `server_credentials` storage with device secure storage.
- [Done safeguard] Disable credential persistence in the mobile UI and backend
  until Keychain/Keystore support is available.
- [Pending] Migrate credentials by stable server ID rather than mutable server name.
- [Done] Enable SQLite foreign keys and repair orphan cleanup.
- [Done] Add revisions, tombstones, bootstrap, and the transactional outbox.

### Phase 1: Mobile terminal MVP

- [Done] Add explicit runtime capabilities.
- [Done] Target-gate local shell, CLI IPC, Agent Gateway, desktop updater, single
  instance, desktop file dialogs, and desktop window controls.
- [Partial] The Apple project is generated and its Rust simulator target checks.
  Android project generation awaits developer acceptance of the Android SDK
  license and NDK 29 installation.
- [Done] Add dynamic viewport and safe-area handling, a touch terminal key bar, visible
  session actions, and full-screen mobile SFTP.
- [Done fallback] Mobile users can paste OpenSSH private keys. Native document
  import remains pending.

### Phase 2: Encrypted metadata sync

- [Done] Implement vault creation, device pairing, AES-256-GCM batch encryption,
  and GitHub Gist and WebDAV transports.
- [Done] Sync servers, groups, and snippets. Portable preferences remain pending.
- [Done] Expose last-success, pending-change, conflict, and error status.
- [Done while unlocked] Sync explicitly, immediately after create/join, on
  foreground/online resume, on a foreground interval, and after debounced GUI
  mutations. App-start auto-unlock still requires secure key persistence.

### Phase 3: Mobile file workflow

- Add platform document-picker adapters for upload and export/share for download.
- Make SFTP list rows touch-sized with single-tap navigation and explicit
  selection mode.
- Add native lifecycle reconnect behavior without claiming background SSH.

### Phase 4: Optional sensitive data

- Add an opt-in credential vault only after local secure storage and recovery
  are independently reviewed.
- Add encrypted recording blob sync with quotas, retention, resumable transfer,
  and explicit deletion semantics.
- Evaluate a separate hosted session relay only if background continuity becomes
  a product requirement.

## Acceptance Gates

- No sync test or payload fixture contains credential, passphrase, private-key,
  fingerprint, recording path, or runtime session fields.
- Concurrent edits and delete-vs-update races converge deterministically on two
  devices, including after retries and out-of-order delivery.
- Killing the app between a domain write and sync cannot lose the outbox entry.
- Applying the same remote batch twice produces the same database state.
- A remote provider object reveals no server inventory or command content.
- Mobile tests cover 320, 375, 390, and 412 CSS-pixel widths, portrait and
  landscape, safe areas, an open software keyboard, Chinese IME, and an external
  keyboard.
- Real-device smoke tests cover SSH connect/reconnect, host-key verification,
  terminal control keys, SFTP navigation, suspend/resume, and offline sync retry
  on both iOS and Android.

## Deployment Decision

Version 1 ships the provider-neutral protocol with private GitHub Gist and
WebDAV adapters. Each user supplies the provider account and credentials;
VibeShell does not operate a synchronization backend. Both adapters retain the
same client-side encryption and conflict semantics.

iCloud-only or Google-Drive-only storage is not used because it breaks
cross-platform pairing and couples the sync model to one mobile ecosystem.
