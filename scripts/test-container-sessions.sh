#!/bin/bash
# Opt-in isolated Docker-in-Docker + SSH + CLI acceptance fixture.
# Never mounts the host Docker socket or connects to saved production servers.
set -euo pipefail
cd "$(dirname "$0")/.."
fixture=$(mktemp -d "${TMPDIR:-/tmp}/vibeshell-container-fixture.XXXXXX")
chmod 700 "$fixture"
mkdir -p "$fixture/ipc"
export XDG_RUNTIME_DIR="$fixture/ipc"
container=""
cleanup() {
  if [ -n "$container" ]; then docker rm -f "$container" >/dev/null; fi
}
trap cleanup EXIT
docker build --tag vibeshell-container-fixture --file scripts/container-fixture/Dockerfile scripts/container-fixture
docker pull busybox:1.37
container=$(docker run --detach --privileged --publish 127.0.0.1::22 vibeshell-container-fixture)
for attempt in $(seq 1 120); do
  if docker exec "$container" docker info >/dev/null 2>&1; then break; fi
  sleep 1
done
docker exec "$container" docker info >/dev/null
docker image save busybox:1.37 | docker exec -i "$container" docker image load >/dev/null
export VIBESHELL_CONTAINER_A=$(docker exec "$container" docker run -d --network none --hostname fixture-alpha --name alpha busybox:1.37 sleep 600)
export VIBESHELL_CONTAINER_B=$(docker exec "$container" docker run -d --network none --hostname fixture-beta --name beta busybox:1.37 sleep 600)
export VIBESHELL_CONTAINER_STOPPED=$(docker exec "$container" docker create --network none --name stopped busybox:1.37 sleep 600)
docker exec "$container" cat /etc/ssh/ssh_host_ed25519_key.pub > "$fixture/host.pub"
export SSH_TEST_HOST=127.0.0.1 SSH_TEST_USER=audit SSH_TEST_PASSWORD=isolated-container-test-only
export SSH_TEST_PORT=$(docker port "$container" 22/tcp | awk -F: '{print $NF}')
export SSH_TEST_FIXTURE_DIR="$fixture"
export VIBESHELL_IPC_NAME="test-containers-$$.sock"
cargo build --locked -p vshell --bin vibeshell
export VIBESHELL_TEST_CLI="$PWD/target/debug/vibeshell"
if ! cargo test --locked -p vibeshell-desktop --lib real_containers_keep_terminal_exec_plugins_cli_and_mcp_in_one_scope -- --ignored --nocapture --test-threads=1; then
  docker logs "$container" > "$fixture/sshd.log" 2>&1
  echo "Isolated fixture diagnostics: $fixture/sshd.log" >&2
  exit 1
fi
