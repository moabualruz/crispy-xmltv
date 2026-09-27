#!/usr/bin/env bash
set -euo pipefail

workflow="$(cd "$(dirname "$0")/.." && pwd)/.github/workflows/ci.yml"
guard="$(awk '
  $0 == "      - name: Verify broker-prepared same-repository source" { step = 1; next }
  step && $0 == "        run: |" { run = 1; next }
  step && /^      - / { exit }
  run { sub(/^          /, ""); print }
' "$workflow")"
test -n "$guard"
grep -Fq 'persist-credentials: false' "$workflow"
grep -Fq 'github.sha' "$workflow"
grep -Fq 'pr-{0}-{1}' "$workflow"
if grep -Fq 'pr-{0}-{1}-run-' "$workflow"; then
  echo 'trusted PR runner label must stay stable across runs' >&2
  exit 1
fi
grep -Fq 'always()' "$workflow"
grep -Fq "group: pr-\${{ github.repository_id }}-\${{ github.event.pull_request.number || github.ref }}" "$workflow"
grep -Fq 'cancel-in-progress: false' "$workflow"
if grep -Fq 'actions/upload-artifact' "$workflow"; then
  echo 'fork gates must run in the checked-out workspace without uploading source' >&2
  exit 1
fi
if grep -Fq 'actions/download-artifact' "$workflow"; then
  echo 'fork gates must run in the checked-out workspace without downloading source' >&2
  exit 1
fi

# Every PR consumer must use the broker attached to GITHUB_WORKSPACE. A checkout
# action is permitted only for forks or non-PR events.
awk '
  function finish_step() {
    if (checkout && condition !~ /github.event_name != .pull_request./ && condition !~ /head.repo.full_name != github.repository/) {
      print "checkout step is not excluded from same-repository PRs" > "/dev/stderr"
      failed = 1
    }
  }
  /^      - / {
    finish_step()
    checkout = ($0 ~ /uses: actions\/checkout@/)
    condition = ""
  }
  checkout && /^        if:/ { condition = $0 }
  END {
    finish_step()
    exit failed
  }
' "$workflow"
test "$(grep -c 'uses: actions/checkout@' "$workflow")" -gt 0

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
route="$(awk '
  $0 == "      - id: route" { step = 1; next }
  step && $0 == "        run: |" { run = 1; next }
  step && /^      - / { exit }
  run { sub(/^          /, ""); print }
' "$workflow")"
test -n "$route"
for attempt in 1 2; do
  output="$tmp/route-$attempt"
  env EVENT_NAME=pull_request HEAD_REPOSITORY=owner/repo BASE_REPOSITORY=owner/repo \
    REPOSITORY_ID=23 PR_NUMBER=17 RUN_ID="$attempt" RUN_ATTEMPT="$attempt" \
    GITHUB_OUTPUT="$output" bash -e -c "$route"
  test "$(cat "$output")" = 'runner_labels=["self-hosted","linux","x64","generic","pr-23-17"]'
done
cmp "$tmp/route-1" "$tmp/route-2"
env EVENT_NAME=pull_request HEAD_REPOSITORY=fork/repo BASE_REPOSITORY=owner/repo \
  REPOSITORY_ID=23 PR_NUMBER=17 GITHUB_OUTPUT="$tmp/fork-route" bash -e -c "$route"
test "$(cat "$tmp/fork-route")" = 'runner_labels=["ubuntu-latest"]'

git -C "$tmp" init -q
git -C "$tmp" config user.name 'Runner contract test'
git -C "$tmp" config user.email 'runner-contract@example.invalid'
printf 'merge source\n' > "$tmp/source.txt"
git -C "$tmp" add source.txt
git -C "$tmp" commit -qm 'merge commit'
merge_sha="$(git -C "$tmp" rev-parse HEAD)"
ln -s "$tmp" "$tmp/workspace"
git clone -q "$tmp" "$tmp/unrelated"
test "$(git -C "$tmp/unrelated" rev-parse HEAD)" = "$merge_sha"
ln -s "$tmp/unrelated" "$tmp/unrelated-workspace"

(
  cd "$tmp"
  GITHUB_WORKSPACE="$tmp/workspace" GITHUB_SHA="$merge_sha" bash -e -c "$guard"
)
if (
  cd "$tmp"
  GITHUB_WORKSPACE="$tmp/workspace" GITHUB_SHA="$(printf '%040d' 0)" bash -e -c "$guard"
); then
  echo 'same-repo guard accepted a checkout at the wrong commit' >&2
  exit 1
fi
if (
  cd "$tmp"
  GITHUB_WORKSPACE="$tmp/unrelated-workspace" GITHUB_SHA="$merge_sha" bash -e -c "$guard"
); then
  echo 'same-repo guard accepted an unrelated workspace at the expected commit' >&2
  exit 1
fi
echo 'runner workflow contract passed'
