# Preparation output formats

These are the shared file formats written by preparation scouts (Phase 3) and read by the act phase (Phase 4).
Phase 3 is the single owner of the schema; Phase 4 links here and must not restate it.

## findings.json (Snoop review)

Written by the Snoop agent to `data/<task>/findings.json`.

```json
{
  "v": 1,
  "pr_url": "https://github.com/owner/repo/pull/42",
  "pr_head": "<sha>",
  "verdict": "APPROVE|REQUEST_CHANGES|COMMENT",
  "findings": [
    {
      "id": "f1",
      "severity": "blocking|nit|question",
      "path": "src/x.rs",
      "line": 42,
      "side": "RIGHT",
      "body": "…"
    }
  ]
}
```

`path` and `line` may be `null` for a general finding not tied to a specific line; such a finding goes in the review body rather than as a line comment.

## triage.json (Triage of feedback on my PR)

Written by the Bunk (Triage) agent to `data/<task>/triage.json`.

```json
{
  "v": 1,
  "pr_url": "https://github.com/owner/repo/pull/42",
  "pr_head": "<sha>",
  "items": [
    {
      "id": "t1",
      "thread_id": "<GraphQL review thread id or null>",
      "comment_id": 12345,
      "action": "fix|reply|pushback|followup",
      "summary": "…",
      "draft_reply": "… or null",
      "fix_plan": "… or null",
      "ticket": {
        "summary": "…",
        "description": "…"
      }
    }
  ]
}
```

`thread_id` is the GraphQL review thread id when the item came from a review thread, or `null` for a standalone PR comment.
`comment_id` is the REST comment id, or `null` when not applicable.
`ticket` is present only for `followup` items.

## review-edits/<task>.json (captain edits made in vessel)

Written by the vessel TUI to `data/vessel/review-edits/<task>.json` when the captain edits a finding before publishing.

```json
{
  "v": 1,
  "findings": {
    "f1": {
      "body": "…",
      "drop": false
    }
  }
}
```

`drop: true` removes a finding from the published review.
This file is read by `vessel/bin/vessel-publish-review.sh` (Phase 4) before publishing.
