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

if (cd "$tmp" && echo dirty >> source.txt && GITHUB_WORKSPACE="$tmp/workspace" GITHUB_SHA="$source_sha" bash -e -c "$guard"); then
  echo 'same-repository guard accepted a dirty worktree' >&2
  exit 1
fi
git -C "$tmp" checkout -q -- source.txt
mkdir "$tmp/plain"
if (cd "$tmp" && GITHUB_WORKSPACE="$tmp/plain" GITHUB_SHA="$source_sha" bash -e -c "$guard"); then
  echo 'same-repository guard accepted a non-symlink workspace' >&2
  exit 1
fi

# Evaluate the real runs-on expression per event (fork PRs must never reach self-hosted).
python3 - "$workflow" <<'PY'
import json, re, sys
from types import SimpleNamespace as N
line = next(l for l in open(sys.argv[1]) if l.startswith("    runs-on: ${{ fromJSON("))
expr = line.split("${{", 1)[1].rsplit("}}", 1)[0].strip().replace("&&", " and ").replace("||", " or ")
def route(name, head="o/r", num=7):
    g = N(event_name=name, repository="o/r", repository_id=42,
          event=N(pull_request=N(head=N(repo=N(full_name=head)), number=num)))
    return eval(expr, {"__builtins__": {}}, {"github": g, "fromJSON": json.loads,
                "format": lambda t, *a: t.format(*a)})
assert route("pull_request") == ["self-hosted", "linux", "x64", "generic", "pr-42-7"], route("pull_request")
assert route("pull_request", head="fork/r") == ["ubuntu-latest"]
assert route("push") == ["self-hosted", "linux", "x64", "generic"]
assert route("workflow_dispatch") == ["self-hosted", "linux", "x64", "generic"]
PY

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
