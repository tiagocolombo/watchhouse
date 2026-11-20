#!/usr/bin/env bash
# Only for the ephemeral Linux CI runner: delegate one unique, initially empty subtree.
set -euo pipefail
model_adapter="${1:-codex}"
case "$model_adapter" in codex|claude) ;; *) echo 'Unknown text adapter'; exit 1 ;; esac
[[ "$(id -u)" != 0 ]] || { echo 'Run this drill as the unprivileged worker'; exit 1; }
model_cgroup="/sys/fs/cgroup/watchhouse-ci-$(python3 -c 'import uuid;print(uuid.uuid4())')"
[[ -f /sys/fs/cgroup/cgroup.controllers ]]
for controller in cpu memory pids; do
  [[ " $(cat /sys/fs/cgroup/cgroup.subtree_control) " == *" $controller "* ]] || { echo "Parent has not delegated $controller; no global controller configuration will be changed"; exit 1; }
done
sudo mkdir "$model_cgroup"
cleanup() {
  if [[ -d "$model_cgroup" ]]; then
    sudo bash -c 'printf 1 > "$1/cgroup.kill"' bash "$model_cgroup" || true
    # Only directories beneath this unique CI-owned subtree, never the hierarchy root.
    sudo find "$model_cgroup" -depth -type d -exec rmdir '{}' \; || true
  fi
}
trap cleanup EXIT
sudo bash -c 'printf "+cpu +memory +pids" > "$1/cgroup.subtree_control"' bash "$model_cgroup"
sudo mkdir "$model_cgroup/coordinator"
sudo chown -R "$(id -u):$(id -g)" "$model_cgroup"
chmod 700 "$model_cgroup"
export WATCHHOUSE_MODEL_CGROUP="$model_cgroup"
export WATCHHOUSE_MODEL_INIT="$(pwd)/target/debug/watchhouse-model-init"
export WATCHHOUSE_MODEL_CODEX
if [[ "$model_adapter" = codex ]]; then
WATCHHOUSE_MODEL_CODEX="$(python3 - <<'PY'
import os,pathlib
root=pathlib.Path(os.environ['RUNNER_TEMP'])/'watchhouse-codex-runtime/node_modules'
files=[p for p in root.rglob('codex') if p.is_file() and p.open('rb').read(4)==b'\x7fELF' and 'x86_64' in str(p)]
assert len(files)==1, 'Exactly one pinned native Codex executable is required'
print(files[0].resolve())
PY
)"
else
  WATCHHOUSE_MODEL_CODEX="$(realpath "$RUNNER_TEMP/watchhouse-claude-runtime/node_modules/@anthropic-ai/claude-code-linux-x64/claude")"
fi
model_test="model_runtime::tests::native_${model_adapter}_cgroup_qualification_and_coordinator_offline_watchdog"
# Linux migration requires write authority at the source/destination common ancestor.
# Bootstrap only this CI worker into its own delegated coordinator child, then drop UID.
# The parent shell stays outside so its trap can remove the entire owned subtree safely.
cargo_path="$(command -v cargo)"
sudo bash -c '
  printf "%s" "$$" > "$1/coordinator/cgroup.procs"
  exec runuser -u "$2" -- env -i HOME="$3" PATH="$4" \
    WATCHHOUSE_MODEL_CGROUP="$1" WATCHHOUSE_MODEL_CODEX="$5" WATCHHOUSE_MODEL_CLAUDE="$5" WATCHHOUSE_MODEL_INIT="$6" \
    "$7" test --locked --lib "$8" -- --ignored --nocapture
' bash "$model_cgroup" "$(id -un)" "$HOME" "$PATH" "$WATCHHOUSE_MODEL_CODEX" "$WATCHHOUSE_MODEL_INIT" "$cargo_path" "$model_test"
