#!/usr/bin/env bash
# Regression test: Radar autoprep max_concurrent accounting counts every
# automatic preparation type (triage, diagnose, review, plan).
#
# Finding P2: section 9.2 counted only triage/diagnose runs, so active Review
# and Plan preparation runs launched automatically by the Radar loop did not
# count toward the cap.
set -u

# shellcheck source=tests/lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

SKILL="$ROOT/.agents/skills/vessel-workflows/SKILL.md"
TMP_ROOT=$(fm_test_tmproot fm-vessel-autoprep-concurrency)

# 1. The documented counting rule names all four automatic preparation types.
COUNTING_RULE=$(sed -n '/^### 9.2 Read autoprep config/,/^### 9.3/p' "$SKILL")
for workflow in triage diagnose review plan; do
  assert_contains "$COUNTING_RULE" "\`$workflow\`" \
    "autoprep counting rule does not count active $workflow runs"
done

# 2. Executable coverage: with Review and Plan runs active at the configured
# limit, the documented rule reports the cap as reached.
cat >"$TMP_ROOT/runs.jsonl" <<'JSON'
{"v":1,"task":"review-webapp-42","workflow":"review","radar_event":"evt-1"}
{"v":1,"task":"plan-proj-7","workflow":"plan","radar_event":"evt-2"}
{"v":1,"task":"triage-webapp-43","workflow":"triage","radar_event":"evt-3"}
{"v":1,"type":"update","task":"diagnose-webapp-40","state":"handed-off"}
{"v":1,"task":"diagnose-webapp-40","workflow":"diagnose","radar_event":"evt-0"}
{"v":1,"task":"implement-webapp-39","workflow":"implement"}
JSON

count_active() {
  jq -s --argjson max "$1" '
    . as $runs
    | ([$runs[] | select(.type != "update")] | map(select(
      (.workflow == "triage" or .workflow == "diagnose"
        or .workflow == "review" or .workflow == "plan")
      and (.task as $t | ([$runs[] | select(.type == "update" and .task == $t and .state == "handed-off")] | length) == 0)
    )) | length) as $active
    | {active: $active, capped: ($active >= $max)}' "$TMP_ROOT/runs.jsonl"
}

RESULT=$(count_active 3)
[ "$(printf '%s' "$RESULT" | jq -r .active)" = "3" ] \
  || fail "expected 3 active preparation runs, got: $RESULT"
[ "$(printf '%s' "$RESULT" | jq -r .capped)" = "true" ] \
  || fail "cap of 3 should be reached with review+plan+triage active: $RESULT"

# Handed-off diagnose run stays excluded, non-prep implement run stays excluded.
RESULT_ROOMY=$(count_active 4)
[ "$(printf '%s' "$RESULT_ROOMY" | jq -r .capped)" = "false" ] \
  || fail "cap of 4 should not be reached: $RESULT_ROOMY"

# 3. The old triage/diagnose-only rule would have missed this: it sees only the
# single triage run and wrongly reports room under the same cap.
OLD_COUNT=$(jq -s '. as $runs | [$runs[] | select(.type != "update")
  | select(.workflow == "triage" or .workflow == "diagnose")
  | .task as $t | select(([$runs[] | select(.type == "update" and .task == $t and .state == "handed-off")] | length) == 0)] | length' \
  "$TMP_ROOT/runs.jsonl")
[ "$OLD_COUNT" = "1" ] \
  || fail "fixture should expose the old undercount (expected 1, got $OLD_COUNT)"

pass "autoprep accounting counts triage, diagnose, review, and plan toward max_concurrent"
