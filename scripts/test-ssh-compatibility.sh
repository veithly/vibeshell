#!/bin/bash
# Runs only the opt-in fixture tests against an isolated, loopback-only container.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p target/ponytail-audit
fixture=$(mktemp -d "$PWD/target/ponytail-audit/ssh.XXXXXX")
chmod 700 "$fixture"
container=""
cleanup() { if [ -n "$container" ]; then docker rm -f "$container" >/dev/null; fi; }
trap cleanup EXIT
for spec in ed25519 rsa ecdsa; do
  ssh-keygen -q -t "$spec" -N '' -f "$fixture/$spec"
done
ssh-keygen -q -t ed25519 -N 'fixture-key-passphrase' -f "$fixture/encrypted"
ssh-keygen -q -t rsa -m PEM -N '' -f "$fixture/rsa-pem"
cat "$fixture/ed25519.pub" "$fixture/rsa.pub" "$fixture/ecdsa.pub" "$fixture/encrypted.pub" "$fixture/rsa-pem.pub" > "$fixture/authorized_keys"
docker build --tag vibeshell-ponytail-sshd --file scripts/ssh-fixture/Dockerfile scripts/ssh-fixture
container=$(docker run --detach --publish 127.0.0.1::22 --publish 127.0.0.1::23 --publish 127.0.0.1::24 vibeshell-ponytail-sshd)
docker cp "$fixture/authorized_keys" "$container:/home/audit/.ssh/authorized_keys"
docker exec "$container" sh -c 'chown audit:audit /home/audit/.ssh/authorized_keys; chmod 600 /home/audit/.ssh/authorized_keys'
docker exec "$container" cat /etc/ssh/ssh_host_ed25519_key.pub > "$fixture/host.pub"
docker exec "$container" cat /etc/ssh/ssh_host_rsa_key.pub > "$fixture/host-rsa.pub"
export SSH_TEST_HOST=127.0.0.1 SSH_TEST_USER=audit SSH_TEST_PASSWORD=ponytail-isolated-test-only
export SSH_TEST_PORT=$(docker port "$container" 22/tcp | awk -F: '{print $NF}')
export SSH_TEST_PAM_PORT=$(docker port "$container" 23/tcp | awk -F: '{print $NF}')
export SSH_TEST_COMPAT_PORT=$(docker port "$container" 24/tcp | awk -F: '{print $NF}')
export SSH_TEST_FIXTURE_DIR="$fixture"
export RUST_LOG=debug
# Log fixture identity, not credentials or private key material.
printf 'OpenSSH fixture ports: normal=%s PAM=%s compatibility=%s\n' "$SSH_TEST_PORT" "$SSH_TEST_PAM_PORT" "$SSH_TEST_COMPAT_PORT"
if ! cargo test -p vibeshell-desktop --test ssh_integration_test -- --ignored --nocapture --test-threads=1; then
  docker logs "$container" > "$fixture/sshd-failure.log" 2>&1
  echo "Fixture server diagnostics: $fixture/sshd-failure.log" >&2
  exit 1
fi
