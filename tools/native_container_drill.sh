#!/usr/bin/env bash
# Explicit native CI integration drill. No model, credentials, network-enabled workload, or publishing.
set -euo pipefail
cd "$(dirname "$0")/.."
container_client="$(command -v docker)"
container_client="$(readlink -f "$container_client")"
drill_root="$(mktemp -d)"
drill_tag="watchhouse-boundary-ci:${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}"
cleanup() {
  "$container_client" image rm "$drill_tag" >/dev/null 2>&1 || true
  rm -rf "$drill_root"
}
trap cleanup EXIT
RUSTFLAGS='-C target-feature=+crt-static' cargo build --locked --target x86_64-unknown-linux-gnu --bin watchhouse-worker-init
cp target/x86_64-unknown-linux-gnu/debug/watchhouse-worker-init "$drill_root/watchhouse-worker-init"
strip "$drill_root/watchhouse-worker-init"
cat > "$drill_root/Dockerfile" <<'DOCKER'
FROM scratch
COPY --chmod=755 watchhouse-worker-init /watchhouse-worker-init
ENTRYPOINT ["/watchhouse-worker-init"]
DOCKER
"$container_client" build --network=none --pull=false --tag "$drill_tag" "$drill_root"
export WATCHHOUSE_DOCKER_PATH="$container_client"
export WATCHHOUSE_TEST_IMAGE
WATCHHOUSE_TEST_IMAGE="$("$container_client" image inspect --format '{{.Id}}' "$drill_tag")"
export WATCHHOUSE_TEST_RUNNER="$drill_root/watchhouse-worker-init"
cargo test --locked --lib real_execution::tests::controller::physical_container_boundary_and_regression -- --ignored --exact --nocapture
