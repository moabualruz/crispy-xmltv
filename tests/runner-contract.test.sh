#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
workflow="$root/.github/workflows/ci.yml"
guard="$(awk '
  $0 == "      - name: Verify broker-prepared same-repository source" { step = 1; next }
  step && $0 == "        run: |" { run = 1; next }
  step && /^      - / { exit }
  run { sub(/^          /, ""); print }
' "$workflow")"
required="$(awk '
  $0 == "      - name: Require the CI gate job to succeed" { step = 1; next }
  step && $0 == "        run: test \"$CI_RESULT\" = success" { print "test \"$CI_RESULT\" = success"; exit }
' "$workflow")"
test -n "$guard"
test -n "$required"
test "$(grep -c 'uses: actions/checkout@' "$workflow")" -eq 1
test "$(grep -c 'uses: dtolnay/rust-toolchain@' "$workflow")" -eq 1
grep -Fq 'needs: [ci]' "$workflow"
grep -Fq 'persist-credentials: false' "$workflow"
grep -Fq 'cancel-in-progress: false' "$workflow"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
git -C "$tmp" init -q
git -C "$tmp" config user.name 'Runner contract test'
git -C "$tmp" config user.email 'runner-contract@example.invalid'
printf 'PR source\n' > "$tmp/source.txt"
git -C "$tmp" add source.txt
git -C "$tmp" commit -qm 'PR source'
source_sha="$(git -C "$tmp" rev-parse HEAD)"
ln -s "$tmp" "$tmp/workspace"
git clone -q "$tmp" "$tmp/unrelated"
ln -s "$tmp/unrelated" "$tmp/unrelated-workspace"
(
  cd "$tmp"
  GITHUB_WORKSPACE="$tmp/workspace" GITHUB_SHA="$source_sha" bash -e -c "$guard"
)
if (cd "$tmp" && GITHUB_WORKSPACE="$tmp/workspace" GITHUB_SHA="$(printf '%040d' 0)" bash -e -c "$guard"); then
  echo 'same-repository guard accepted the wrong commit' >&2
  exit 1
fi
if (cd "$tmp" && GITHUB_WORKSPACE="$tmp/unrelated-workspace" GITHUB_SHA="$source_sha" bash -e -c "$guard"); then
  echo 'same-repository guard accepted an unrelated checkout' >&2
  exit 1
fi

env CI_RESULT=success bash -e -c "$required"
if env CI_RESULT=failure bash -e -c "$required"; then
  echo 'required status accepted a failed CI job' >&2
  exit 1
fi

mkdir "$tmp/bin"
cat > "$tmp/bin/cargo" <<'SH'
#!/usr/bin/env bash
printf '%s\t%s\n' "$*" "$CARGO_TARGET_DIR" >> "$GATE_LOG"
[[ "${FAIL_GATE:-}" != "${1:-}" ]]
SH
cat > "$tmp/bin/bash" <<'SH'
#!/bin/bash
if [[ "${1:-}" == tests/runner-contract.test.sh ]]; then exit 0; fi
exec /bin/bash "$@"
SH
chmod +x "$tmp/bin/cargo" "$tmp/bin/bash"
if ! PATH="$tmp/bin:$PATH" RUNNER_TEMP="$tmp" GITHUB_RUN_ID=contract GITHUB_RUN_ATTEMPT=1 GATE_LOG="$tmp/gates" /bin/bash "$root/.github/run-ci-gates.sh"; then
  echo 'parallel gate runner failed without an injected failure' >&2
  exit 1
fi
test "$(wc -l < "$tmp/gates")" -eq 5
test "$(cut -f2 "$tmp/gates" | sort -u | wc -l)" -eq 5
if PATH="$tmp/bin:$PATH" RUNNER_TEMP="$tmp" GITHUB_RUN_ID=contract GITHUB_RUN_ATTEMPT=2 GATE_LOG="$tmp/failing-gates" FAIL_GATE=clippy /bin/bash "$root/.github/run-ci-gates.sh"; then
  echo 'parallel gate runner ignored a failing gate' >&2
  exit 1
fi
test "$(wc -l < "$tmp/failing-gates")" -eq 5
echo 'runner workflow contract passed'
