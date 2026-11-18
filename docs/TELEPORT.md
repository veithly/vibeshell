# Teleport connections

Teleport profiles use the current native `vibeshell` CLI and the same session owner as the desktop and MCP. They do not restore the retired `vshell` executable or introduce a second daemon.

## Connect a node

Install a `tsh` client compatible with your cluster on the machine running the VibeShell service. Log in explicitly before connecting; VibeShell does not extract Teleport keys, perform login/MFA, request additional access, or disable certificate checks.

```bash
tsh login --proxy=teleport.example.com:443
vibeshell servers add alice@node-name --type teleport --proxy teleport.example.com:443 --name work-node
vibeshell ssh work-node
vibeshell sessions
```

`alice` is the node's Unix login, not necessarily your Teleport account name. A saved node must identify one machine; label selectors, wildcards and comma-separated targets are rejected. Teleport authenticates using the service user's existing `tsh` profile. Password environment variables and SSH private-key options are not used for this transport.

In the desktop Add Server dialog, select **Teleport**, then enter the node, Unix login and proxy. Edit Server can change these values without loading SSH credentials. Create a separate profile to change between SSH and Teleport. Restart/upgrade the GUI and CLI together when upgrading the shared service.

## Import nodes explicitly

```bash
vibeshell import teleport --proxy teleport.example.com:443 --user alice --dry-run --json
vibeshell import teleport --proxy teleport.example.com:443 --user alice
```

Import reads bounded `tsh ls --format=json` output and prefers the node's resource identity to a potentially duplicated hostname. It does not guess an operating-system login from a Teleport account. `import auto` does not invoke `tsh`, contact a cluster or open an authentication prompt. `--path` is for file-based SSH import, not for a Teleport proxy.

## Terminal, commands and files

The desktop and CLI attach to the same owned terminal. Readiness is confirmed by an authenticated remote command; starting a local `tsh` process is not treated as a successful connection. EOF, authentication errors and cancellation terminate the transport. Quick commands and plugin actions use the same node/login/proxy context and propagate real exit status. Closing the session cancels its running command and file transports; commands are never replayed automatically.

SFTP uses a real binary SFTP protocol channel over `tsh ssh --no-tty`. The node must permit remote execution of a standard OpenSSH `sftp-server` executable in PATH or a supported system location. A node that only allows a built-in SFTP subsystem, but not an external server executable, is not supported by this implementation. VibeShell reports that limitation rather than falling back to `cat`, parsing `ls`, or pretending that recursive `scp` is directory synchronization.

On supported nodes, file operations use the ordinary shared SFTP implementation: binary-safe bytes and sizes, exact Unicode/whitespace filenames, exclusive creation when overwrite is refused, and directory sync with exclusions, nested `.gitignore` rules, explicit deletion policy and actual transfer counts. An unchanged sync reports no uploads, transferred bytes or newly created directories.

Subprocess input/output is bounded and drained concurrently. Commands have deadlines; the PTY startup deadline is 30 seconds, SFTP negotiation is bounded by 20 seconds, and SFTP responses by 15 seconds. On cancellation the subprocess is terminated; an already queued request in the current SFTP library may take the remainder of its response deadline to report failure.

## Scope and verification

This transport supports desktop platforms with a compatible installed `tsh`. Mobile Teleport sessions, using a Teleport profile as a raw SSH jump host, raw SSH port/agent-forwarding options and SSH tunnels through a Teleport session are not supported. These operations must not silently fall back to an unrelated SSH host or credential.

Automated regressions use isolated subprocesses, a `tsh`-shaped fixture and real SFTP protocol messages, not user profiles or cluster credentials. They cover authentication failure, PTY readiness/EOF, input/output deadlocks and limits, cancellation, shared-session command dispatch, binary transfers, overwrite refusal, exact paths, ignore retention and sync counts. A real cluster's client version, access policy, MFA configuration and availability of an external SFTP server still require environment-specific acceptance testing.

## Outbound network policy

The profile proxy above is a Teleport cluster address, not the application HTTP/SOCKS proxy. The external `tsh` process cannot enforce VibeShell's native outbound route; when such a route is active, Teleport connections/exec/import/SFTP fail explicitly before starting a process rather than bypassing it. Native SSH supports the configured route. See [network scope](NETWORK_AND_CONTAINERS.md).
