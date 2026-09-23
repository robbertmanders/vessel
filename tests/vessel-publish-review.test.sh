#!/usr/bin/env bash
# tests/vessel-publish-review.test.sh - fail-closed preflight lookups in
# vessel/bin/vessel-publish-review.sh.
#
# A failed or empty PR-head, authenticated-user, or pending-review lookup must
# stop publication before any review POST. A fake `gh` on PATH records every
# review-POST attempt; each case asserts the script exits nonzero and the POST
# log stays empty.
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT/vessel/bin/vessel-publish-review.sh"

FAILED=0
fail() { printf 'not ok - %s\n' "$1" >&2; FAILED=1; }
pass() { printf 'ok - %s\n' "$1"; }

# Build a fixture home with a minimal findings.json, plus a fake gh whose
# behaviour is driven by env vars. Echoes the fixture dir and fakebin.
#   FAKE_HEAD / FAKE_HEAD_RC        - `gh pr view` stdout / exit code
#   FAKE_USER / FAKE_USER_RC        - `gh api /user` stdout / exit code
#   FAKE_PENDING / FAKE_PENDING_RC  - `gh api .../reviews` stdout / exit code
setup_world() {
  local dir fakebin
  dir=$(mktemp -d "${TMPDIR:-/tmp}/vessel-pubrev.XXXXXX")
  fakebin="$dir/fakebin"
  mkdir -p "$fakebin" "$dir/data/task1"
  cat > "$dir/data/task1/findings.json" <<'EOF'
{
  "pr_url": "https://github.com/acme/webapp/pull/7",
  "pr_head": "abc123",
  "verdict": "COMMENT",
  "findings": []
}
EOF
  cat > "$fakebin/gh" <<'EOF'
#!/usr/bin/env bash
# Fake gh: preflight answers come from env; any review POST is logged.
if [ "${1:-}" = "pr" ]; then
  printf '%s' "${FAKE_HEAD:-}"
  exit "${FAKE_HEAD_RC:-0}"
fi
if [ "${1:-}" = "api" ] && [ "${2:-}" = "/user" ]; then
  printf '%s' "${FAKE_USER:-}"
  exit "${FAKE_USER_RC:-0}"
fi
if [ "${1:-}" = "api" ]; then
  if [ "${2:-}" = "--method" ]; then
    echo "POSTED" >> "$FAKE_POST_LOG"
    printf '{"id": 1, "html_url": "https://github.com/acme/webapp/pull/7#review-1"}'
    exit 0
  fi
  for a in "$@"; do
    case "$a" in
      *"/reviews"*)
        printf '%s' "${FAKE_PENDING:-}"
        exit "${FAKE_PENDING_RC:-0}"
        ;;
    esac
  done
fi
echo "fake gh: unexpected invocation: $*" >&2
exit 99
EOF
  chmod +x "$fakebin/gh"
  touch "$dir/post.log"
  printf '%s %s\n' "$dir" "$fakebin"
}

# Each case runs the script once with its env overrides and asserts a nonzero
# exit with no review POST. Defaults model the healthy preflight
# (matching head, known user, no pending review); each case overrides one
# lookup toward failure or emptiness.
one_case() { # <name> <KEY=val...> (space-separated assignments, "-" for none)
  local name=$1 assigns=$2
  local dir fakebin rc
  read -r dir fakebin < <(setup_world)
  # shellcheck disable=SC2086
  PATH="$fakebin:$PATH" FAKE_POST_LOG="$dir/post.log" FM_HOME="$dir" \
  FAKE_HEAD=abc123 FAKE_USER=captain FAKE_PENDING=0 env $assigns \
    "$SCRIPT" --task task1 --yes >/dev/null 2>&1
  rc=$?
  if [ "$rc" -eq 0 ]; then
    fail "$name refuses (exited 0)"
  elif [ -s "$dir/post.log" ]; then
    fail "$name posts no review"
  else
    pass "$name"
  fi
  rm -rf "$dir"
}

one_case "failed PR-head lookup stops the publish" "FAKE_HEAD_RC=1 FAKE_HEAD="
one_case "empty PR-head lookup stops the publish" "FAKE_HEAD= FAKE_HEAD_RC=0"
one_case "moved PR head stops the publish" "FAKE_HEAD=def456"
one_case "failed authenticated-user lookup stops the publish" "FAKE_USER_RC=1 FAKE_USER="
one_case "empty authenticated-user lookup stops the publish" "FAKE_USER= FAKE_USER_RC=0"
one_case "failed pending-review lookup stops the publish" "FAKE_PENDING_RC=1 FAKE_PENDING="
one_case "empty pending-review lookup stops the publish" "FAKE_PENDING= FAKE_PENDING_RC=0"
one_case "existing pending review stops the publish" "FAKE_PENDING=1"

# Happy path: matching head, known user, no pending review -> review POSTs.
happy_path_posts_the_review() {
  local dir fakebin rc
  read -r dir fakebin < <(setup_world)
  PATH="$fakebin:$PATH" FAKE_POST_LOG="$dir/post.log" FM_HOME="$dir" \
  FAKE_HEAD=abc123 FAKE_USER=captain FAKE_PENDING=0 \
    "$SCRIPT" --task task1 --yes >/dev/null 2>&1
  rc=$?
  if [ "$rc" -ne 0 ]; then
    fail "healthy preflight publishes (exit $rc)"
  elif [ ! -s "$dir/post.log" ]; then
    fail "healthy preflight publishes (no POST recorded)"
  else
    pass "healthy preflight publishes"
  fi
  rm -rf "$dir"
}
happy_path_posts_the_review

# Missing pr_head in findings.json stops the publish without any POST.
missing_review_head_stops_the_publish() {
  local dir fakebin rc
  read -r dir fakebin < <(setup_world)
  printf '{"pr_url": "https://github.com/acme/webapp/pull/7", "verdict": "COMMENT", "findings": []}' \
    > "$dir/data/task1/findings.json"
  PATH="$fakebin:$PATH" FAKE_POST_LOG="$dir/post.log" FM_HOME="$dir" \
  FAKE_HEAD=abc123 FAKE_USER=captain FAKE_PENDING=0 \
    "$SCRIPT" --task task1 --yes >/dev/null 2>&1
  rc=$?
  if [ "$rc" -eq 0 ]; then
    fail "missing reviewed head stops the publish (exited 0)"
  elif [ -s "$dir/post.log" ]; then
    fail "missing reviewed head stops the publish (review POSTed)"
  else
    pass "missing reviewed head stops the publish"
  fi
  rm -rf "$dir"
}
missing_review_head_stops_the_publish

if [ "$FAILED" -ne 0 ]; then
  echo "FAILED" >&2
  exit 1
fi
echo "All vessel-publish-review preflight tests passed."
