#!/usr/bin/env bash
# vessel-jira.sh - Wrap acli jira workitem for configured vessel transitions.
#
# Reads jira_transitions from config/vessel/vessel.json for logical name resolution.
# Default transitions: {"pickup": "In Progress", "pr_open": "In Review"}.
#
# Prints what it will do and requires --yes to act.
#
# Usage:
#   vessel-jira.sh pickup     --ticket <KEY> [--home <fm-home>] [--yes]
#   vessel-jira.sh transition --ticket <KEY> --to <name-or-status> [--home <fm-home>] [--yes]
#   vessel-jira.sh comment    --ticket <KEY> --body <text> [--home <fm-home>] [--yes]
#   vessel-jira.sh create     --ticket <PARENT-KEY> --summary <text>
#                               [--description <text>] [--type <type>]
#                               [--home <fm-home>] [--yes]
#   vessel-jira.sh -h|--help
set -eu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FM_HOME="${FM_HOME:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

usage() { sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//' >&2; exit "${1:-0}"; }

# Default transition names used when config/vessel/vessel.json has no jira_transitions.
DEFAULT_PICKUP="In Progress"
DEFAULT_PR_OPEN="In Review"

# Read a transition name from config/vessel/vessel.json, falling back to the default.
read_transition() {
  local key="$1" default="$2"
  local config_file="$FM_HOME/config/vessel/vessel.json"
  if [ -f "$config_file" ] && command -v jq >/dev/null 2>&1; then
    local value
    value=$(jq -r --arg k "$key" '.jira_transitions[$k] // empty' "$config_file" 2>/dev/null || true)
    [ -n "$value" ] && echo "$value" && return
  fi
  echo "$default"
}

[ $# -eq 0 ] && { usage 2; }
subcommand="$1"; shift

ticket="" to_status="" body="" summary="" description="" ticket_type="Task" yes=0

while [ $# -gt 0 ]; do
  case "$1" in
    --ticket)       ticket="$2";       shift 2 ;;
    --home)         FM_HOME="$2";      shift 2 ;;
    --to)           to_status="$2";    shift 2 ;;
    --body)         body="$2";         shift 2 ;;
    --summary)      summary="$2";      shift 2 ;;
    --description)  description="$2";  shift 2 ;;
    --type)         ticket_type="$2";  shift 2 ;;
    --yes)          yes=1;             shift   ;;
    -h|--help)      usage 0 ;;
    *) echo "vessel-jira: unknown argument: $1" >&2; usage 2 ;;
  esac
done

command -v acli >/dev/null || { echo "vessel-jira: acli is required" >&2; exit 1; }
command -v jq   >/dev/null || { echo "vessel-jira: jq is required (for config reading)" >&2; exit 1; }

case "$subcommand" in

  pickup)
    if [ -z "$ticket" ]; then
      echo "vessel-jira pickup: --ticket is required" >&2; exit 2
    fi
    pickup_status=$(read_transition "pickup" "$DEFAULT_PICKUP")
    echo "=== vessel-jira pickup: dry run ==="
    echo "  ticket:   $ticket"
    echo "  assign:   @me"
    echo "  transition: -> $pickup_status"
    echo ""
    if [ "$yes" -ne 1 ]; then
      echo "Dry run only. Pass --yes to act."
      exit 0
    fi
    echo "Assigning $ticket to self..."
    acli jira workitem assign --key "$ticket" --assignee "@me"
    echo "Transitioning $ticket to '$pickup_status'..."
    acli jira workitem transition --key "$ticket" --status "$pickup_status" --yes
    echo "Done."
    ;;

  transition)
    if [ -z "$ticket" ]; then
      echo "vessel-jira transition: --ticket is required" >&2; exit 2
    fi
    if [ -z "$to_status" ]; then
      echo "vessel-jira transition: --to is required" >&2; exit 2
    fi
    # Resolve a logical name (pickup, pr_open) or use the value as a literal status.
    resolved_status=$(read_transition "$to_status" "")
    [ -z "$resolved_status" ] && resolved_status="$to_status"
    echo "=== vessel-jira transition: dry run ==="
    echo "  ticket:     $ticket"
    echo "  transition: -> $resolved_status"
    echo ""
    if [ "$yes" -ne 1 ]; then
      echo "Dry run only. Pass --yes to act."
      exit 0
    fi
    echo "Transitioning $ticket to '$resolved_status'..."
    acli jira workitem transition --key "$ticket" --status "$resolved_status" --yes
    echo "Done."
    ;;

  comment)
    if [ -z "$ticket" ]; then
      echo "vessel-jira comment: --ticket is required" >&2; exit 2
    fi
    if [ -z "$body" ]; then
      echo "vessel-jira comment: --body is required" >&2; exit 2
    fi
    echo "=== vessel-jira comment: dry run ==="
    echo "  ticket: $ticket"
    echo "  body:   ${body:0:80}$([ "${#body}" -gt 80 ] && echo '...' || true)"
    echo ""
    if [ "$yes" -ne 1 ]; then
      echo "Dry run only. Pass --yes to act."
      exit 0
    fi
    echo "Posting comment on $ticket..."
    acli jira workitem comment create --key "$ticket" --body "$body"
    echo "Done."
    ;;

  create)
    if [ -z "$ticket" ]; then
      echo "vessel-jira create: --ticket (parent key) is required" >&2; exit 2
    fi
    if [ -z "$summary" ]; then
      echo "vessel-jira create: --summary is required" >&2; exit 2
    fi
    # Derive the project key from the parent ticket key (e.g. "AA4FI-123" -> "AA4FI").
    project=$(echo "$ticket" | sed 's/-[0-9]*$//')
    echo "=== vessel-jira create: dry run ==="
    echo "  parent:  $ticket"
    echo "  project: $project"
    echo "  type:    $ticket_type"
    echo "  summary: $summary"
    if [ -n "$description" ]; then
      echo "  description: ${description:0:80}$([ "${#description}" -gt 80 ] && echo '...' || true)"
    fi
    echo ""
    if [ "$yes" -ne 1 ]; then
      echo "Dry run only. Pass --yes to act."
      exit 0
    fi
    echo "Creating follow-up ticket under $ticket..."
    if [ -n "$description" ]; then
      acli jira workitem create \
        --project "$project" \
        --type "$ticket_type" \
        --summary "$summary" \
        --description "$description" \
        --parent "$ticket"
    else
      acli jira workitem create \
        --project "$project" \
        --type "$ticket_type" \
        --summary "$summary" \
        --parent "$ticket"
    fi
    echo "Done."
    ;;

  -h|--help) usage 0 ;;
  *) echo "vessel-jira: unknown subcommand: $subcommand" >&2; usage 2 ;;
esac
