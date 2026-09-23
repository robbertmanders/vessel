# vessel preparation file formats

These are the authoritative schemas for the files that Phase 3 scouts write and Phase 4 outward scripts consume.
Phase 3 (`vessel-workflows` radar loop and scout contracts) writes them.
Phase 4 (`vessel-publish-review.sh`, `vessel-reply.sh`) reads them.

## `data/<task>/findings.json` (Snoop review output)

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
      "body": "comment text"
    }
  ]
}
```

`path` and `line` may be `null` for a general finding, which goes in the overall review body rather than as a line comment.
`side` is `"RIGHT"` (new version) or `"LEFT"` (old version); omit or null for general findings.

## `data/<task>/triage.json` (Triage of feedback on my PR)

```json
{
  "v": 1,
  "pr_url": "https://github.com/owner/repo/pull/42",
  "pr_head": "<sha>",
  "items": [
    {
      "id": "t1",
      "thread_id": "<GraphQL review thread id or null>",
      "comment_id": 123456789,
      "action": "fix|reply|pushback|followup",
      "summary": "short description",
      "draft_reply": "reply text or null",
      "fix_plan": "implementation notes or null",
      "ticket": {"summary": "ticket title", "description": "ticket body"} 
    }
  ]
}
```

`comment_id` is the REST API comment id (integer or null).
`thread_id` is the GraphQL `PullRequestReviewThread.id` (string or null).
`draft_reply` is non-null for `action: reply` and `action: pushback`.
`fix_plan` is non-null for `action: fix`.
`ticket` is non-null for `action: followup`.

## `data/vessel/review-edits/<task>.json` (captain edits made in vessel)

```json
{
  "v": 1,
  "findings": {
    "f1": {"body": "edited comment text", "drop": false}
  }
}
```

Keys are finding `id` values from `findings.json`.
`body` replaces the finding's body when submitting.
`drop: true` excludes the finding from the submitted review.
Absent keys mean no edit: use the original finding unchanged.
