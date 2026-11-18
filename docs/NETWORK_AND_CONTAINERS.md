# Container sessions, outbound routes and Gateway ports

These features are implemented in the development source for VibeShell 1.2.
An already installed binary is not updated by merging source. The executable
remains `vibeshell`; the retired `vshell` executable is not restored.

## Persistent Docker sessions — #9

Connect to an SSH host, then choose **More → Docker containers** in the tab-bar
actions. The panel lists running containers first and keeps stopped/other states
in a collapsed section. Refresh reports connection, Docker and permission errors;
it does not poll every host automatically, invoke sudo or start stopped containers.

Click a running container to open its own terminal tab/pane. Optional Unix user,
absolute working directory and shell selection apply to that child. The default
shell is `/bin/sh`; Bash, Ash and Zsh must actually exist in the image.

```bash
vibeshell containers list --session 001
vibeshell containers open --session 001 CONTAINER_ID --workdir /app
vibeshell sessions
vibeshell attach CHILD_SESSION_ID
vibeshell ssh-session CHILD_SESSION_ID -- hostname
```

The saved context records the owner, parent SSH session and full immutable Docker
ID. Reusing a container name does not retarget an existing tab. Interactive CLI
input shares that child's PTY, including `cd` and `export`. Independent MCP exec,
quick commands and plugin actions run inside the same container/user with the
configured working directory, not with the interactive shell's transient variables.
MCP exposes `container_list` and `container_open`; owner routing also works when
the desktop is attached to a headless daemon. Guards are evaluated by the owner.
A headless owner cannot grant interactive approval: requests needing an approval
UI fail closed rather than accepting an agent-provided approval flag.

Closing a child closes its channel and cancels its captured command operations,
not the container or parent connection. Closing the parent disconnects its
children. Container exit or shell EOF marks the child disconnected. Reconnect
returns to container selection and never converts the tab into a host shell.

**Supported scope:** direct SSH parents with Docker CLI/daemon permissions.
Local-shell and Teleport parents, nested child sessions, container SFTP, and
container-native SSH tunnels/database probes are not implemented. Unsupported
file requests fail explicitly; the host filesystem is never substituted. Use
commands in the container for its files. Containers without a compatible shell
produce a startup error rather than a misleading connected tab.

## Outbound network route — #12

Settings → **Outbound proxy** provides Direct, System environment and Custom.
The same device-local configuration is available to the native CLI and daemon:

```bash
vibeshell proxy show
vibeshell proxy set --mode custom --url socks5h://127.0.0.1:1080
vibeshell proxy set --mode system
vibeshell proxy set --mode direct
```

Use the password field in Settings, or `--username` with `--password-stdin` from
a protected pipe. Password arguments and credentials in Custom-mode URLs are rejected; standard System-mode environment URLs can contain encoded credentials. Input
bytes, including spaces and any trailing newline, are preserved exactly. An
omitted password keeps the encrypted value; `--clear-password` removes it.
Changing the proxy endpoint or username requires explicitly replacing or clearing
the old password so it cannot silently be sent to another proxy.

| Protocol | Authentication | Destination DNS / address support |
| --- | --- | --- |
| HTTP | Optional Basic proxy authentication | CONNECT target name is sent to the proxy; IPv4/IPv6 authority syntax |
| HTTPS | The same, inside verified TLS to the proxy | Proxy certificate chain and hostname must validate against public WebPKI roots |
| SOCKS4 | Optional user ID; no password authentication | Local DNS, IPv4 only |
| SOCKS4a | Optional user ID; no password authentication | Proxy DNS, IPv4 destinations |
| SOCKS5 | No-auth or username/password | Local DNS; IPv4 and IPv6 |
| SOCKS5h | No-auth or username/password | Proxy DNS for names; IPv4 and IPv6 literals |

System environment mode reads lower/uppercase `ALL_PROXY`, `HTTPS_PROXY`,
`HTTP_PROXY` and `NO_PROXY` from the process environment. Raw TCP chooses ALL,
then HTTPS, then HTTP. `NO_PROXY` supports `*`, host/IP names and domain suffixes;
CIDR and host:port patterns are not interpreted. This mode does not read macOS or
Windows proxy-control-panel preferences. Direct explicitly ignores proxy environment
variables. Custom does not apply environment bypass lists.

### Routing and safety boundaries

New SSH host-key probes/authentication (including the first jump-host connection),
SFTP over those connections, native PostgreSQL/MySQL connections, cloud-sync HTTP,
AI prediction requests, explicitly approved remote Markdown images, update metadata, signed updater downloads and reverse
forwarding destination sockets use the shared policy. Host-key verification,
updater signature verification and HTTPS verification remain enabled. Fixed
deadlines cover proxy negotiation; cancellation drops the socket. Authentication,
TLS, DNS, refusal and timeout errors never trigger a direct retry.

Changing the policy affects new connections/requests, not live SSH channels or
an updater download whose client was already created. Reconnect active sessions
to change their route. Local per-user IPC and the loopback Agent Gateway are not
outbound server connections and remain local. The native HTTP bridge bounds
request/response size, cancels pending work and never retries through WebView fetch. Remote Markdown images still require explicit consent and are fetched through the native route as bounded raster data, not directly by the WebView. External CSS supplied by the user and the system browser are outside native routing.

Commands running on remote hosts, externally launched coding agents, and the
system browser manage their own networking; this is not a machine-wide VPN.
The integrated external Teleport `tsh` transport cannot enforce this native
route, so it refuses to start while an outbound proxy is active instead of
silently bypassing it. Teleport's per-profile `--proxy` is a different setting.
Self-signed HTTPS proxy certificates are rejected; there is no insecure switch.

## Persistent Agent Gateway port — #19

Settings → **Agent Gateway → Gateway port** accepts `0` for automatic allocation
or a fixed integer from 1 through 65535. Saving persists the preference; save
ongoing work and restart VibeShell to apply it. The panel distinguishes the
running endpoint from the saved preference.

The Gateway still binds only to `127.0.0.1`, rotates its authentication token,
and publishes its discovery manifest. If a chosen port is occupied or prohibited
by the OS, the desktop stays open and displays an actionable error in Settings.
It does not elevate, select another port or advertise a successful connection.

## Verification

Standard gates include frontend build/tests, Rust workspace tests/check/strict
Clippy, formatting, release metadata and generated plugin references. Loopback
fixtures cover CONNECT framing/refusal, SOCKS4a/5h DNS, exact proxy credentials,
HTTPS trusted/untrusted certificates and hostname mismatch, cancellation and
no-direct-fallback behavior. Gateway tests cover persistence across starts and
occupied/invalid ports. Frontend tests cover explicit saves and failure states.

`bash scripts/test-container-sessions.sh` is an opt-in real Docker-in-Docker
acceptance fixture. It uses a disposable privileged test container, a separate
nested Docker daemon and generated test host keys; it never mounts the host
Docker socket. It verifies two real container PTYs, CLI/MCP/plugin execution,
daemon ownership, shell state, stopped refusal, command cancellation, child
teardown and parent disconnect. The Rust test runs as a unit-test module with
in-memory test cryptography, a temporary database and a separate IPC directory.
The fixture does not read real saved servers or device credential keys.

Linux manual CI also runs this fixture and the existing isolated OpenSSH suite.
The synthetic proxy TLS CA/private key are test-only and must never be installed
as system trust or used for production services.
