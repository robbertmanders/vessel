#!/usr/bin/env bash
# vessel-reply.sh - Post accepted draft replies to PR threads and re-request review.
#
# Reads data/<task>/triage.json (written by a Triage scout) and posts draft_reply
# values for items with action=reply or action=pushback to the identified threads.
# Optionally re-requests review from reviewers already on the PR.
#
# File format reference: vessel/docs/prep-formats.md
#
# Prints what it will do and requires --yes to act.
#
# Usage:
#   vessel-reply.sh --task <id> [--home <fm-home>]
#     [--items <id,...>]
#     [--no-rerequest]
#     [--yes]
#   vessel-reply.sh -h|--help
set -eu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FM_HOME="${FM_HOME:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

usage() { sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//' >&2; exit "${1:-0}"; }

task="" items_filter="" no_rerequest=0 yes=0

while [ $# -gt 0 ]; do
  case "$1" in
    --task)         task="$2";          shift 2 ;;
    --home)         FM_HOME="$2";       shift 2 ;;
    --items)        items_filter="$2";  shift 2 ;;
    --no-rerequest) no_rerequest=1;     shift   ;;
    --yes)          yes=1;              shift   ;;
    -h|--help)      usage 0 ;;
    *) echo "vessel-reply: unknown argument: $1" >&2; usage 2 ;;
  esac
done

if [ -z "$task" ]; then
  echo "vessel-reply: --task is required" >&2
  usage 2
fi

command -v jq >/dev/null || { echo "vessel-reply: jq is required" >&2; exit 1; }
command -v gh >/dev/null || { echo "vessel-reply: gh is required" >&2; exit 1; }

triage_file="$FM_HOME/data/$task/triage.json"

if [ ! -f "$triage_file" ]; then
  echo "vessel-reply: $triage_file not found" >&2
  echo "Run a Triage scout first to generate triage.json." >&2
  exit 1
fi

pr_url=$(jq -r '.pr_url' "$triage_file")
if [ -z "$pr_url" ] || [ "$pr_url" = "null" ]; then
  echo "vessel-reply: triage.json has no pr_url" >&2
  exit 1
fi

owner_repo=$(echo "$pr_url" | sed 's|https://github.com/||' | cut -d/ -f1-2)
pr_number=$(echo "$pr_url" | grep -oE '/pull/[0-9]+' | grep -oE '[0-9]+')

# Select items with a draft reply, filtered by --items if given.
reply_items=$(jq -c \
  --arg filter "$items_filter" \
  '[.items[] |
     select(.draft_reply != null and .draft_reply != "") |
     select(.action == "reply" or .action == "pushback") |
     if $filter != "" then
       select([$filter | split(",")[] | . == ..id] | any)
     else . end
   ]' "$triage_file")

reply_count=$(echo "$reply_items" | jq 'length')

echo "=== vessel-reply: dry run ==="
echo "  PR:     $pr_url"
echo "  replies: $reply_count"
echo ""

if [ "$reply_count" -eq 0 ]; then
  echo "No items with draft replies. Nothing to post."
  if [ "$yes" -ne 1 ]; then exit 0; fi
fi

# Print each reply.
echo "$reply_items" | jq -c '.[]' | while IFS= read -r item; do
  id=$(echo "$item" | jq -r '.id')
  action=$(echo "$item" | jq -r '.action')
  summary=$(echo "$item" | jq -r '.summary')
  draft=$(echo "$item" | jq -r '.draft_reply')
  thread_id=$(echo "$item" | jq -r '.thread_id // empty')
  comment_id=$(echo "$item" | jq -r '.comment_id // empty')
  anchor="${thread_id:-comment $comment_id}"
  echo "  [$action $id] $summary"
  echo "    thread/comment: $anchor"
  echo "    reply: ${draft:0:80}$([ "${#draft}" -gt 80 ] && echo '...' || true)"
  echo ""
done

if [ "$no_rerequest" -eq 0 ]; then
  echo "  Will re-request review from current review requestees."
  echo ""
fi

if [ "$yes" -ne 1 ]; then
  echo "Dry run only. Pass --yes to post these replies."
  exit 0
fi

# Post each reply.
echo "$reply_items" | jq -c '.[]' | while IFS= read -r item; do
  id=$(echo "$item" | jq -r '.id')
  draft=$(echo "$item" | jq -r '.draft_reply')
  thread_id=$(echo "$item" | jq -r '.thread_id // empty')
  comment_id_val=$(echo "$item" | jq -r '.comment_id // empty')

  if [ -n "$comment_id_val" ] && [ "$comment_id_val" != "null" ]; then
    # Reply to a review thread comment via REST.
    gh api --method POST \
      "/repos/$owner_repo/pulls/comments/$comment_id_val/replies" \
      -f body="$draft" >/dev/null \
      && echo "  posted reply to comment $comment_id_val ($id)" \
      || echo "  warning: could not reply to comment $comment_id_val ($id)" >&2
  elif [ -n "$thread_id" ] && [ "$thread_id" != "null" ]; then
    # For a GraphQL thread id, fetch the first comment id and reply to it.
    first_comment_id=$(gh api graphql \
      -f query='query($id: ID!) {
        node(id: $id) {
          ... on PullRequestReviewThread {
            comments(first: 1) { nodes { databaseId } }
          }
        }
      }' -f id="$thread_id" \
      --jq '.data.node.comments.nodes[0].databaseId' 2>/dev/null || true)
    if [ -n "$first_comment_id" ] && [ "$first_comment_id" != "null" ]; then
      gh api --method POST \
        "/repos/$owner_repo/pulls/comments/$first_comment_id/replies" \
        -f body="$draft" >/dev/null \
        && echo "  posted reply to thread $thread_id ($id)" \
        || echo "  warning: could not reply to thread $thread_id ($id)" >&2
    else
      echo "  warning: could not find comment id for thread $thread_id ($id)" >&2
    fi
  else
    echo "  warning: item $id has no thread_id or comment_id, skipping" >&2
  fi
done

# Re-request review unless suppressed.
if [ "$no_rerequest" -eq 0 ]; then
  reviewers=$(gh pr view "$pr_url" --json reviewRequests \
    --jq '[.reviewRequests[].login] | join(",")' 2>/dev/null || true)
  if [ -n "$reviewers" ] && [ "$reviewers" != "" ]; then
    gh pr request-review "$pr_url" --reviewer "$reviewers" \
      && echo "Re-requested review from: $reviewers" \
      || echo "warning: could not re-request review" >&2
  else
    echo "(no existing review requestees to re-request)"
  fi
fi

echo "Done."
