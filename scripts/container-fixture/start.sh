#!/bin/sh
set -eu
ssh-keygen -A
cat > /etc/ssh/sshd_config <<'EOF'
Port 22
ListenAddress 0.0.0.0
PasswordAuthentication yes
PermitRootLogin no
UsePAM no
AllowUsers audit
Subsystem sftp internal-sftp
EOF
# This is an isolated nested daemon. No host Docker socket or host filesystem
# is mounted. It has no externally exposed API and no network for child nodes.
dockerd --host=unix:///var/run/docker.sock --storage-driver=vfs --bridge=none --iptables=false --ip-forward=false >/tmp/fixture-dockerd.log 2>&1 &
for attempt in $(seq 1 120); do
  if docker info >/dev/null 2>&1; then exec /usr/sbin/sshd -D -e; fi
  sleep 1
done
cat /tmp/fixture-dockerd.log >&2
exit 1
