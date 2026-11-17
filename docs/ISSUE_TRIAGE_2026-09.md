# September 2026 PR and issue review

Reviewed on **2026-09-29**. This document distinguishes implemented changes from proposed work. The application version remains 1.1.0; changing source does not update an installed application or publish a release.

## Pull requests

| PR | Decision | Evidence and remaining work |
| --- | --- | --- |
| [#15](https://github.com/veithly/vibeshell/pull/15) | Merged into `main` | Workflow-only annotated-tag checkout and release metadata fixes; all required checks passed. |
| [#16](https://github.com/veithly/vibeshell/pull/16) | Merged into `dev` | Three-language product tour and five screenshots of real components using synthetic data. The fixture is a separate development-only loopback entry; all required checks passed. |
| [#10](https://github.com/veithly/vibeshell/pull/10) | Keep open; changes required | Head `e8fd8d0` has not changed since the September 18 review and conflicts with `dev`. Do not merge by merely resolving textual conflicts. |
| [#11](https://github.com/veithly/vibeshell/pull/11) | Keep open; changes required | Head `b81e609` is unchanged since review, conflicts with `dev`, and includes #10. Session and file-operation contracts still need implementation and regression fixtures. |

### #10: preserve CLI usability without credential regressions

The headless create/delete workflow is useful. The blockers are in the implementation, not in the feature:

- `cli/src/commands/server.rs::env_nonempty` and `commands/server.rs::add_server_spec` trim passwords/passphrases. Presence checks must not modify secret bytes, including whitespace-only secrets.
- Creating a group, server and encrypted credentials must be one database transaction, including associations and sync outbox changes. Inject a failure at each mutation and assert rollback. Deleting credentials before a failing metadata delete must not leave a broken saved server.
- `AddServerSpec` derives `Debug` and carries credentials; the new IPC variant falls through the existing log-redaction match. Add sentinel tests for password, key material and passphrase, including activity/error paths.
- The proposed GUI adapter drops `ServerInput.credential_id` when constructing `AddServerSpec`. Preserve existing credential associations and add a GUI-backend regression test.
- The new delete CLI currently has no confirmation flag or prompt. Require explicit confirmation, with a deliberate noninteractive option; refuse ambiguous names. Invalid `host:port` inputs must fail rather than becoming literal hostnames.

Rebase on `dev`, reuse the current transactional storage/credential paths, then run parser, rollback, redaction and full workspace checks. No unsafe contributor code was executed as part of this review.

### #11: split transport support from unimplemented file semantics

First resolve #10. A smaller initial Teleport PR should establish a reliable session lifecycle and capability reporting. File features may explicitly report unsupported until their normal contracts are met.

Use asynchronously drained, bounded stdout/stderr, concurrent stdin, timeouts and cancellation for `tsh`. The current synchronous `Command.output()` and write-before-drain paths can hang or exhaust memory. Authentication failures and EOF must drive real state transitions; process spawn or a fixed sleep is not connection success. Preserve GUI/daemon ownership, quick commands, plugin execution and teardown.

Do not replace exclusive file creation with `cat >`, binary reads with lossy text, or structured directories with trimmed `ls` lines. Recursive `scp` is not directory synchronization: exclusions, nested ignore rules, deletion policy and actual transfer counts must be honored. Fixtures should cover binary bytes, whitespace/Unicode names, overwrite refusal, ignored-file retention, failed authentication, cancellation and bounded process I/O.

## #9: Docker containers as independent sessions

Issue: [#9](https://github.com/veithly/vibeshell/issues/9).

### Implemented in this change

The Docker Containers plugin now starts with a **Running containers** action and has a separate **Exited containers** action. Both use full container IDs and expose machine-readable state alongside status. A **Container inventory (JSON Lines)** action returns one Docker-formatted JSON object per line for agent consumers. The existing all-container action remains available; these queries do not start containers or elevate automatically.

Container operands are separated from Docker options with `--`; inspect is restricted to container objects. Existing mutation confirmations and optional sudo controls remain. Plugin tests cover inventory filters, full IDs, read-only defaults and option-boundary protection. Generated references are shared by all three Skill distributions.

This is inventory groundwork, **not** one-click container sessions. There is no new collapsible sidebar or container PTY in this change. `exec-command` remains a single noninteractive command.

### Required next implementation

Add an explicit container context to a child session: owning process, parent SSH server/session identity, immutable full container ID, selected user and working directory. Do not identify a live session only by a reusable container name.

The interactive PTY and every independent command path must use that context. Today `Session::exec_command_with_stdin`, quick commands and plugin operations open SSH exec channels on the host. Merely sending `docker exec -it ...` into a terminal would leave agents executing on the host. Route GUI, CLI, MCP, quick commands and plugin execution consistently, and show the container context in tabs and activity records.

Closing the container session must close its exec process, not stop the container or kill the parent session. Container termination must disconnect the child explicitly, never silently leave a host shell under a container label. Check actual exec success; preserve resize, input, cancellation and output replay. A stopped container stays stopped unless the user explicitly authorizes starting it. Shell selection must handle images without Bash or any shell with a clear error.

Initially reject container-file operations that are not implemented. Host SFTP must never be presented as the container filesystem. A later file transport needs binary-safe reads, exclusive writes, path validation and real sync semantics.

The UI should fetch inventory when the container panel opens, with refresh and visible permission/connection errors. Show running entries first and collapsed exited entries; use the full ID when opening a new tab. Avoid automatic polling or sudo on every SSH connection. Test two containers concurrently, agent-created tab synchronization, host/container execution identity, container stop/restart, parent disconnect and independent child cleanup using isolated fixtures.

## #12: outbound proxy support

Issue: [#12](https://github.com/veithly/vibeshell/issues/12). **Design only; proxy protocols are not implemented by this change.** Existing SSH SOCKS forwarding is a listener for traffic through an established SSH connection, not an outbound proxy setting.

### Scope and existing network paths

| Path | Current implementation | Required integration |
| --- | --- | --- |
| SSH host-key probe and authentication | Shared `SshClient::establish_connection` in `src-tauri/src/ssh/client.rs` | Establish the selected proxy tunnel before SSH; use the same route for probe and authentication. Preserve TOFU and hostname/port identity. |
| SSH exec, SFTP and forwarding | Channels on established SSH connections | Reuse the proxied transport rather than opening accidental direct connections. Include jump-host entry connections. |
| Cloud sync | `reqwest::Client` in `src-tauri/src/cloud_sync/providers.rs` | Apply the same policy, credential handling and bypass rules. |
| Model prediction and update metadata | Frontend `fetch` in `src/lib/aiCommandPrediction.ts` and `src/stores/updateStore.ts` | Route through a controlled native HTTP path or explicitly configured WebView networking; a Rust environment variable alone does not configure browser fetch. |
| Native updater, external agents and remote commands | Separate networking owners | Audit updater downloads separately. Locally launched tools and commands running on a remote host must have explicit scope; do not claim they inherit a global proxy automatically. |

### Proposed delivery order and safety contract

1. Define a shared proxy configuration with explicit direct/system/custom modes and per-server overrides. Store proxy credentials with the existing encrypted credential mechanism, not in URLs, command arguments or activity logs. Preserve exact password bytes; provide environment/stdin or a credential reference rather than `--proxy-auth user:pass` in argv.
2. Implement a timeout/cancellation-aware tunnel dialer for HTTP CONNECT and SOCKS5. Reuse the SSH stream connection entry point so host-key checks precede authentication. Distinguish local and proxy-side DNS and document supported authentication. No automatic direct fallback when a proxy is configured.
3. Add HTTPS-proxy TLS verification and SOCKS4/4a with an explicit DNS/IPv6 compatibility matrix. Do not disable certificate verification to support a corporate proxy; expose a deliberate trust configuration. Unsupported destination/protocol combinations must fail clearly.
4. Apply the policy to the HTTP and updater paths above before advertising application-wide coverage. Expose unsupported/external traffic honestly. Redact proxy credentials in connection errors and give actionable authentication, certificate, DNS and timeout diagnostics.

Use loopback proxy fixtures with generated credentials. Cover CONNECT refusal, SOCKS authentication, malformed replies, timeout, cancellation, IPv4/IPv6, local versus proxy DNS, TLS rejection, exact credential bytes and GUI/CLI parity. Assert that an unavailable proxy never causes a direct connection. No real saved servers or live proxy credentials belong in these tests.

## Integration and release boundaries

Prioritize #10's credential-safe foundation and #9's shared execution-context design before adding another independent transport. Proxy support can then use the shared connection boundary; Teleport needs its own bounded process transport rather than pretending to be raw SSH/SFTP.

CI compilation and releases are now explicitly manual; all required merge checks remain enforced. See [contribution checks](../CONTRIBUTING.md#manual-github-checks) and [release procedure](RELEASING.md). This review does not close #9 or #12, publish a new version, install a new binary, or terminate user sessions.
