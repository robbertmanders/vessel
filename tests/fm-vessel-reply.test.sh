#!/usr/bin/env bash
# Tests for vessel/bin/vessel-reply.sh failure accounting: a failed reply,
# reviewer re-request, or GraphQL lookup must drive a nonzero exit and name
# what failed, while successes are reported so partial outcomes can be
# reconciled without duplicating already-posted replies on retry.
set -u

# shellcheck source=tests/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

REPLY="$ROOT/vessel/bin/vessel-reply.sh"
TMP_ROOT=$(fm_test_tmproot fm-vessel-reply-tests)
BASE_PATH=$PATH

# Build a case home with a triage.json holding two reply items and a fake gh.
# Echoes the case directory. Fake behavior is driven by env files in the case:
#   gh-fail-ids       comma list of comment databaseIds whose POST fails
#   gh-graphql-fail   when present, the thread-id lookup returns null
#   gh-rerequest-fail when present, `gh pr request-review` fails
make_case() {
  local name=$1 case_dir fakebin home
  case_dir="$TMP_ROOT/$name"
  fakebin="$case_dir/fakebin"
  home="$case_dir/home"
  mkdir -p "$fakebin" "$home/data/task-1"
  cat > "$home/data/task-1/triage.json" <<'JSON'
{
  "pr_url": "https://github.com/acme/webapp/pull/42",
  "items": [
    {"id": "c1", "action": "reply", "summary": "first thread",
     "draft_reply": "thanks, fixed", "comment_id": "111"},
    {"id": "c2", "action": "pushback", "summary": "second thread",
     "draft_reply": "holding this line", "comment_id": "222"}
  ]
}
JSON
  cat > "$fakebin/gh" <<'SH'
#!/usr/bin/env bash
set -u
case_dir="${FM_VESSEL_REPLY_CASE:?}"
fail_ids=""
[ -f "$case_dir/gh-fail-ids" ] && fail_ids=$(cat "$case_dir/gh-fail-ids")
if [ "${1:-}" = "api" ]; then
  shift
  for arg in "$@"; do
    case "$arg" in
      graphql)
        if [ -f "$case_dir/gh-graphql-fail" ]; then
          echo "null"
        else
          echo "999"
        fi
        exit 0
        ;;
      /repos/*/pulls/comments/*/replies)
        id=$(printf '%s' "$arg" | sed 's|.*/comments/||; s|/replies||')
        case ",$fail_ids," in
          *,"$id",*) echo "simulated POST failure for $id" >&2; exit 1 ;;
        esac
        echo '{}'
        exit 0
        ;;
    esac
  done
  echo '{}'
  exit 0
fi
if [ "${1:-}" = "pr" ]; then
  shift
  case "${1:-}" in
    view) echo "alice"; exit 0 ;;
    request-review)
      if [ -f "$case_dir/gh-rerequest-fail" ]; then
        echo "simulated re-request failure" >&2
        exit 1
      fi
      exit 0
      ;;
  esac
fi
echo "fake gh: unexpected args: $*" >&2
exit 1
SH
  chmod +x "$fakebin/gh"
  printf '%s\n' "$case_dir"
}

run_reply() {
  local case_dir=$1
  shift
  FM_VESSEL_REPLY_CASE="$case_dir" PATH="$case_dir/fakebin:$BASE_PATH" \
    bash "$REPLY" --task task-1 --home "$case_dir/home" --yes "$@"
}

test_all_success_exits_zero_with_summary() {
  local dir out code
  dir=$(make_case success)
  out=$(run_reply "$dir" 2>&1); code=$?
  [ "$code" -eq 0 ] || fail "expected exit 0, got $code: $out"
  case "$out" in
    *"posted reply to comment 111 (c1)"*) ;;
    *) fail "missing success line for c1: $out" ;;
  esac
  case "$out" in
    *"posted reply to comment 222 (c2)"*) ;;
    *) fail "missing success line for c2: $out" ;;
  esac
  case "$out" in
    *"posted 2 reply(ies), failed 0"*) ;;
    *) fail "missing posted/failed summary: $out" ;;
  esac
  pass "all-success exits 0 and reports posted replies"
}

test_total_failure_exits_nonzero() {
  local dir out code
  dir=$(make_case total-fail)
  printf '%s' "111,222" > "$dir/gh-fail-ids"
  out=$(run_reply "$dir" 2>&1); code=$?
  [ "$code" -ne 0 ] || fail "expected nonzero exit on total failure, got 0: $out"
  case "$out" in
    *"c1"*c2*|*"c2"*c1*) ;;
    *) fail "error should name both failed items: $out" ;;
  esac
  pass "total reply failure exits nonzero and names failed items"
}

test_partial_failure_reports_posted_and_failed() {
  local dir out code
  dir=$(make_case partial-fail)
  printf '%s' "222" > "$dir/gh-fail-ids"
  out=$(run_reply "$dir" 2>&1); code=$?
  [ "$code" -ne 0 ] || fail "expected nonzero exit on partial failure, got 0: $out"
  case "$out" in
    *"posted reply to comment 111 (c1)"*) ;;
    *) fail "partial outcome must still report the posted reply: $out" ;;
  esac
  case "$out" in
    *"posted 1 reply(ies), failed 1"*) ;;
    *) fail "partial outcome must report posted/failed counts: $out" ;;
  esac
  case "$out" in
    *c2*) ;;
    *) fail "partial outcome must name the failed item c2: $out" ;;
  esac
  case "$out" in
    *"Retry only: c2"*) ;;
    *) fail "partial outcome must scope the retry to c2: $out" ;;
  esac
  pass "partial failure exits nonzero and scopes the retry to failed items"
}

test_rerequest_failure_exits_nonzero() {
  local dir out code
  dir=$(make_case rerequest-fail)
  : > "$dir/gh-rerequest-fail"
  out=$(run_reply "$dir" 2>&1); code=$?
  [ "$code" -ne 0 ] || fail "expected nonzero exit on re-request failure, got 0: $out"
  case "$out" in
    *"re-requesting review failed"*) ;;
    *) fail "re-request failure should be reported as an error: $out" ;;
  esac
  pass "re-request failure exits nonzero"
}

test_graphql_lookup_failure_exits_nonzero() {
  local dir out code
  dir=$(make_case graphql-fail)
  cat > "$dir/home/data/task-1/triage.json" <<'JSON'
{
  "pr_url": "https://github.com/acme/webapp/pull/42",
  "items": [
    {"id": "t1", "action": "reply", "summary": "thread item",
     "draft_reply": "noted", "thread_id": "THREAD123"}
  ]
}
JSON
  : > "$dir/gh-graphql-fail"
  out=$(run_reply "$dir" 2>&1); code=$?
  [ "$code" -ne 0 ] || fail "expected nonzero exit on GraphQL lookup failure, got 0: $out"
  case "$out" in
    *t1*) ;;
    *) fail "lookup failure should name the item: $out" ;;
  esac
  pass "GraphQL lookup failure exits nonzero"
}

test_all_success_exits_zero_with_summary
test_total_failure_exits_nonzero
test_partial_failure_reports_posted_and_failed
test_rerequest_failure_exits_nonzero
test_graphql_lookup_failure_exits_nonzero
