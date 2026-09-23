#!/usr/bin/env bash
# vessel-jira-test.sh - prove the authorized transition path acts while the default stays dry-run.
#
# Usage: vessel/tests/vessel-jira-test.sh
#   Exits 0 when both assertions pass, 1 otherwise. Uses a fake `acli` and a
#   fake `jq` on PATH so no live Jira transition happens.
set -eu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JIARA="$SCRIPT_DIR/../bin/vessel-jira.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/home/config/vessel"
echo '{"jira_transitions":{"pr_open":"In Review"}}' > "$tmp/home/config/vessel/vessel.json"

# Fake jq: answer the transition lookup, nothing else.
cat > "$tmp/bin/jq" <<'EOF'
#!/usr/bin/env bash
# Minimal stand-in: echo the pr_open name for any jira_transitions lookup.
echo "In Review"
EOF
chmod +x "$tmp/bin/jq"

# Fake acli: record invocations instead of calling Jira.
export ACLI_LOG="$tmp/acli.log"
: > "$ACLI_LOG"
cat > "$tmp/bin/acli" <<'EOF'
#!/usr/bin/env bash
echo "$*" >> "$ACLI_LOG"
EOF
chmod +x "$tmp/bin/acli"

export PATH="$tmp/bin:$PATH"

fail() { echo "FAIL: $1" >&2; exit 1; }

# 1. Default (no --yes) must stay dry-run: acli untouched, exit 0.
out=$("$JIARA" transition --ticket AA4FI-1 --to pr_open --home "$tmp/home") \
  || fail "default dry-run exited non-zero"
[ -s "$ACLI_LOG" ] && fail "default dry-run invoked acli: $(cat "$ACLI_LOG")"
echo "$out" | grep -q "Dry run only" || fail "default dry-run printed no dry-run notice"

# 2. Authorized (--yes) path must invoke the intended Jira transition with --yes.
out=$("$JIARA" transition --ticket AA4FI-1 --to pr_open --home "$tmp/home" --yes) \
  || fail "authorized --yes path exited non-zero"
grep -q "jira workitem transition --key AA4FI-1 --status In Review --yes" "$ACLI_LOG" \
  || fail "authorized path did not invoke the intended transition: $(cat "$ACLI_LOG" 2>/dev/null || echo '<empty>')"

echo "PASS: default stays dry-run; --yes invokes the Jira transition"
