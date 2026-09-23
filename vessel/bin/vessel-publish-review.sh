#!/usr/bin/env bash
# vessel-publish-review.sh - Submit a pending GitHub review built from vessel findings.
#
# Reads data/<task>/findings.json (written by a Snoop review scout), applies any
# captain edits from data/vessel/review-edits/<task>.json, and submits a GitHub
# review via the REST API.
#
# File format reference: vessel/docs/prep-formats.md
#
# Refuses when the PR head has moved since the review or when the captain already
# has a pending review on this PR.
# Prints what it will do and requires --yes to act.
#
# Usage:
#   vessel-publish-review.sh --task <id> [--home <fm-home>]
#     [--findings <id,...>]
#     [--submit COMMENT|REQUEST_CHANGES|APPROVE]
#     [--resolve-threads <id,...>]
#     [--yes]
#   vessel-publish-review.sh -h|--help
set -eu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FM_HOME="${FM_HOME:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

usage() { sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//' >&2; exit "${1:-0}"; }

task="" findings_filter="" submit_event="" resolve_threads="" yes=0

while [ $# -gt 0 ]; do
  case "$1" in
    --task)          task="$2";             shift 2 ;;
    --home)          FM_HOME="$2";          shift 2 ;;
    --findings)      findings_filter="$2";  shift 2 ;;
    --submit)        submit_event="$2";     shift 2 ;;
    --resolve-threads) resolve_threads="$2"; shift 2 ;;
    --yes)           yes=1;                 shift   ;;
    -h|--help)       usage 0 ;;
    *) echo "vessel-publish-review: unknown argument: $1" >&2; usage 2 ;;
  esac
done

if [ -z "$task" ]; then
  echo "vessel-publish-review: --task is required" >&2
  usage 2
fi

command -v jq  >/dev/null || { echo "vessel-publish-review: jq is required"  >&2; exit 1; }
command -v gh  >/dev/null || { echo "vessel-publish-review: gh is required"   >&2; exit 1; }

findings_file="$FM_HOME/data/$task/findings.json"
edits_file="$FM_HOME/data/vessel/review-edits/$task.json"

if [ ! -f "$findings_file" ]; then
  echo "vessel-publish-review: $findings_file not found" >&2
  echo "Run a Snoop review scout first to generate findings." >&2
  exit 1
fi

pr_url=$(jq -r '.pr_url' "$findings_file")
review_head=$(jq -r '.pr_head' "$findings_file")
suggested_verdict=$(jq -r '.verdict' "$findings_file")

if [ -z "$pr_url" ] || [ "$pr_url" = "null" ]; then
  echo "vessel-publish-review: findings.json has no pr_url" >&2
  exit 1
fi

# Parse owner/repo and PR number from the URL.
owner_repo=$(echo "$pr_url" | sed 's|https://github.com/||' | cut -d/ -f1-2)
pr_number=$(echo "$pr_url" | grep -oE '/pull/[0-9]+' | grep -oE '[0-9]+')

if [ -z "$owner_repo" ] || [ -z "$pr_number" ]; then
  echo "vessel-publish-review: could not parse owner/repo and PR number from $pr_url" >&2
  exit 1
fi

# Check whether the PR head has moved since the review.
current_head=$(gh pr view "$pr_url" --json headRefOid --jq .headRefOid 2>/dev/null || true)
if [ -n "$current_head" ] && [ "$current_head" != "$review_head" ]; then
  echo "vessel-publish-review: the PR head has moved since this review" >&2
  echo "  reviewed head: $review_head" >&2
  echo "  current head:  $current_head" >&2
  echo "Re-run the review scout on the new head before publishing." >&2
  exit 1
fi

# Check for an existing pending review from the captain.
current_user=$(gh api /user --jq .login 2>/dev/null || true)
if [ -n "$current_user" ]; then
  pending=$(gh api "/repos/$owner_repo/pulls/$pr_number/reviews" \
    --jq "[.[] | select(.state == \"PENDING\" and .user.login == \"$current_user\")] | length" \
    2>/dev/null || echo "0")
  if [ "$pending" != "0" ] && [ "$pending" != "" ]; then
    echo "vessel-publish-review: you already have a pending review on $pr_url" >&2
    echo "Submit or dismiss the existing pending review before creating a new one." >&2
    exit 1
  fi
fi

# Determine the event (verdict).
event="${submit_event:-$suggested_verdict}"
case "$event" in
  APPROVE|REQUEST_CHANGES|COMMENT) ;;
  *) echo "vessel-publish-review: --submit must be APPROVE, REQUEST_CHANGES, or COMMENT (got: $event)" >&2; exit 2 ;;
esac

# Build the list of active findings (applying edits and the id filter).
active_findings=$(jq -c \
  --arg filter "$findings_filter" \
  --argjson edits "$([ -f "$edits_file" ] && jq '.findings // {}' "$edits_file" || echo '{}')" \
  '[.findings[] |
     . as $f |
     ($edits[$f.id] // {}) as $e |
     if ($e.drop // false) then empty
     elif ($filter != "" and ([$filter | split(",")[] | . == $f.id] | any | not)) then empty
     else
       $f +
       (if $e.body then {body: $e.body} else {} end)
     end
   ]' "$findings_file")

general_findings=$(echo "$active_findings" | jq '[.[] | select(.path == null or .path == "")]')
line_findings=$(echo "$active_findings" | jq '[.[] | select(.path != null and .path != "")]')

total=$(echo "$active_findings" | jq 'length')
blocking=$(echo "$active_findings" | jq '[.[] | select(.severity == "blocking")] | length')
nits=$(echo "$active_findings" | jq '[.[] | select(.severity == "nit")] | length')
questions=$(echo "$active_findings" | jq '[.[] | select(.severity == "question")] | length')

# Build overall review body.
body_parts=""
if [ "$blocking" -gt 0 ]; then body_parts="${body_parts:+$body_parts, }$blocking blocking"; fi
if [ "$nits"     -gt 0 ]; then body_parts="${body_parts:+$body_parts, }$nits nit$([ "$nits" -ne 1 ] && echo s || true)"; fi
if [ "$questions" -gt 0 ]; then body_parts="${body_parts:+$body_parts, }$questions question$([ "$questions" -ne 1 ] && echo s || true)"; fi

review_body=""
if [ -n "$body_parts" ]; then
  review_body="Review findings: $body_parts."
fi
# Append general (non-line) findings to the body.
general_count=$(echo "$general_findings" | jq 'length')
if [ "$general_count" -gt 0 ]; then
  while IFS= read -r finding; do
    finding_body=$(echo "$finding" | jq -r '.body')
    review_body="${review_body:+$review_body

}$finding_body"
  done < <(echo "$general_findings" | jq -c '.[]')
fi

# Build line-level comments array.
comments=$(echo "$line_findings" | jq -c \
  '[.[] | {
     path: .path,
     line: (.line // null),
     side: (.side // "RIGHT"),
     body: .body
   }]')

echo "=== vessel-publish-review: dry run ==="
echo "  PR:      $pr_url"
echo "  head:    $review_head"
echo "  event:   $event"
echo "  findings: $total active ($blocking blocking, $nits nits, $questions questions)"
echo "  body:    ${review_body:-<empty>}"
echo ""

if [ "$(echo "$line_findings" | jq 'length')" -gt 0 ]; then
  echo "  Line comments:"
  echo "$line_findings" | jq -r '.[] | "    \(.path):\(.line)  [\(.severity)]  \(.body[:60])"'
  echo ""
fi

if [ -n "$resolve_threads" ] && [ "$resolve_threads" != "" ]; then
  echo "  Threads to resolve after submission:"
  echo "$resolve_threads" | tr ',' '\n' | sed 's/^/    /'
  echo ""
fi

if [ "$yes" -ne 1 ]; then
  echo "Dry run only. Pass --yes to post this review."
  exit 0
fi

# Build the API payload.
payload=$(jq -nc \
  --arg commit_id "$review_head" \
  --arg event "$event" \
  --arg body "$review_body" \
  --argjson comments "$comments" \
  '{commit_id: $commit_id, event: $event, body: $body, comments: $comments}')

echo "Posting review to $pr_url..."
response=$(echo "$payload" | gh api --method POST "/repos/$owner_repo/pulls/$pr_number/reviews" \
  --header "Content-Type: application/json" --input -)
review_id=$(echo "$response" | jq -r '.id // empty')
review_html=$(echo "$response" | jq -r '.html_url // empty')
echo "Review posted: ${review_html:-id $review_id}"

# Resolve threads if requested.
if [ -n "$resolve_threads" ] && [ "$resolve_threads" != "" ]; then
  echo "Resolving threads..."
  IFS=',' read -ra thread_ids <<< "$resolve_threads"
  for thread_id in "${thread_ids[@]}"; do
    thread_id=$(echo "$thread_id" | xargs)
    [ -z "$thread_id" ] && continue
    gh api graphql -f query='mutation($id: ID!) {
      resolveReviewThread(input: {threadId: $id}) { thread { id } }
    }' -f id="$thread_id" >/dev/null \
      && echo "  resolved $thread_id" \
      || echo "  warning: could not resolve $thread_id" >&2
  done
fi

echo "Done."
