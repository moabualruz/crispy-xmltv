#!/usr/bin/env bash
set -euo pipefail

root="$RUNNER_TEMP/cargo-target/$GITHUB_RUN_ID/$GITHUB_RUN_ATTEMPT/ci"
mkdir -p "$root"
pids=()
run_gate() {
  local name=$1
  shift
  (export CARGO_TARGET_DIR="$root/$name"; "$@") &
  pids+=("$!")
}
run_gate fmt cargo fmt --check
run_gate clippy cargo clippy --all-targets --all-features -- -D warnings
run_gate test cargo test --all-features
run_gate doc cargo doc --no-deps
run_gate package cargo package
run_gate contract bash tests/runner-contract.test.sh
status=0
for pid in "${pids[@]}"; do wait "$pid" || status=1; done
exit "$status"
