---
name: vessel-workflows
description: >-
  Captain-installed vessel extension: dispatch Remy-style workflows to the crew with the captain's vessel agents.
  Use when the captain asks to implement or plan a Jira ticket, review a pull request, address PR feedback,
  resolve PR merge conflicts, update a PR description, or create a Jira ticket, including loose references such
  as "this PR", "that ticket", or "my PRs" that point at what the captain sees in vessel; when a request names a vessel
  agent (Snoop, Slim Charles, Proposition Joe, Conflict Resolver, Stringer, Ticket Creator, PR Description, Bunk, Freamon, or
  any agent in config/vessel/agents.json); for "standup" or "what did I do yesterday" (the standup digest, section 11); for every inbox note that starts with `[vessel]`; and on every
  `check: vessel-radar:` notification (the Radar loop, section 9).
user-invocable: false
metadata:
  internal: true
---

# vessel-workflows

This fork of firstmate ships a TUI called vessel (`vessel/`).
The captain defines **agents** in it (name, mode, harness, model, effort, instructions), and each agent does one kind of **workflow**.
This skill turns a workflow request into an ordinary firstmate task, using the agent's profile and instructions.
It adds no new lifecycle: intake, backlog, delivery mode, yolo, supervision, merge, and teardown stay exactly as `AGENTS.md` defines them.
The skill only decides *which* agent, *what* the brief says, and records the run so the TUI can show it under the ticket or PR.

This is a standing captain instruction: when a request matches, the agent's harness, model, and effort are the captain's explicit dispatch choice and win over `config/crew-dispatch.json` rules.

## 1. Parse the request

- Chat: read the captain's words, e.g. "implement AA4FI-1234 with Slim Charles", "review acme/webapp#42", "address the feedback on that PR".
- Inbox note starting with `[vessel]`: parse the canonical grammar in [`vessel/docs/requests.md`](../../../vessel/docs/requests.md). Keep the note's request id; it goes into the run record.

Resolve the **workflow** and the **target**:

| Workflow      | Target          | Agent mode  | Kind                |
| ------------- | --------------- | ----------- | ------------------- |
| `implement`   | Jira key        | Implement   | ship                |
| `plan`        | Jira key        | Plan        | scout               |
| `review`      | pull request    | Review      | scout               |
| `address`     | pull request    | Address     | ship on existing PR |
| `conflicts`   | pull request    | Conflicts   | ship on existing PR |
| `description` | pull request    | Description | scout               |
| `ticket`      | Jira feature    | Ticket      | scout               |
| `free`        | pull request    | none        | captain's prompt    |

### What the captain is looking at

The captain usually has vessel open beside this chat, and it publishes what it shows to `data/vessel/context.json`:

```json
{"v":1,"ts":1790000000,
 "focus":{"kind":"pr","repo":"acme/webapp","number":42,"url":"…","title":"…","ticket_key":"AA4FI-1234","selected_feedback":["<thread id>"]},
 "my_prs":[{"repo","number","title","url","head","status","has_conflicts","needs_attention","ticket_key"}],
 "review_prs":[{"repo","number","title","url","head","my_status","total_status","ticket_key"}],
 "tickets":[{"key","summary","status","feature"}]}
```

`focus` is the open PR or ticket page, the open run (`kind: "run"`, with `task`, `ticket_key`, `repo`, `number`), or the highlighted list row; `null` when vessel shows neither or has quit.
`my_prs` are the captain's own PRs, `review_prs` the PRs waiting on the captain's review, `tickets` the captain's Jira tickets; a missing list was never loaded.
Ignore the file when it is absent or `ts` is more than 15 minutes old (vessel rewrites it at least every 5 minutes while it runs).

When the request names no target, or names it loosely ("this PR", "that ticket", "PR 42", "the rate limiting PR", "Joe's PR"), resolve it from this file before asking:

- "this"/"that"/no target: `focus`, if its kind fits the workflow (a run's `repo`/`number` or `ticket_key` counts).
- A number, key, or title words: the one entry in `review_prs`, `my_prs`, or `tickets` that matches; `focus` breaks a tie.
- `address` with no `comments=`: a non-empty `focus.selected_feedback` is the list of thread or comment ids to address.

Name the resolved target in your acknowledgement ("reviewing acme/webapp#42, the PR open in vessel") so a wrong match is caught at once.
The file is only a pointer: section 4 still fetches the live PR or ticket.
Outside workflow requests, read the same file when the captain refers to "my PRs", "my tickets", or what is on screen, rather than re-querying GitHub or Jira.

If the workflow or target is still ambiguous, ask one concise question instead of guessing, listing the candidates you found.

## 2. Resolve the agent

Read `config/vessel/agents.json` (if it is missing, use `vessel/defaults/agents.json`):

```json
{"version":1,"agents":[{"name":"Snoop","mode":"Review","harness":"pi","model":"openrouter/meta/muse-spark-1.3-contributor","effort":"high","instructions_file":"snoop.md"}]}
```

- An agent the captain named wins (case-insensitive name match).
- Otherwise take the agents whose `mode` matches the workflow's mode, preferring the default names Snoop (Review), Slim Charles (Implement), Proposition Joe (Address), Conflict Resolver (Conflicts), Stringer (Plan), Ticket Creator (Ticket), PR Description (Description); else the first match.
- If no agent has that mode, say so and ask which agent to use; do not fall back silently.
- Instructions are the contents of `config/vessel/agents/<instructions_file>` (or `vessel/defaults/agents/…`). Empty is allowed.
- `model=` / `effort=` in the request override the agent's values for this run only.
- `free` uses no agent: resolve the profile the ordinary way (section 4 of `AGENTS.md`).

## 3. Resolve the project

Read `config/vessel/vessel.json` when it exists:

- `feature_projects`: Jira feature key or name → project name under `projects/`.
- `repo_projects`: GitHub `owner/repo` → project name.

Without a mapping, a PR's repository name matching a registered project is a confident match; a Jira ticket resolves through its feature mapping or the ordinary intake rules.
An unmatched or unregistered repository goes to the captain as an ordinary project-intake question (`project-management` skill).

## 4. Gather context (read-only)

- Jira: `acli jira workitem view <KEY> --fields key,summary,description,comment,parent --json`.
- PR: `gh pr view <url-or-number> -R <owner/repo> --json number,title,url,headRefName,headRefOid,baseRefName,headRepositoryOwner,isCrossRepository,mergeable,body`.
  If the PR title contains a Jira key, also fetch that ticket as ticket context.
- `address`: fetch the requested comments or threads (`gh api graphql` on `reviewThreads`/`comments`, or `gh pr view --comments`). With no `comments=`, take the unresolved review threads. Format each as
  `- kind: <review summary|review thread|standalone PR comment>` followed by `id`, `url`, `location: <path>:<line>`, `author`, `body` lines.
- `plan=<task>` (implement): the plan is `data/<task>/report.md`.

Never mutate Jira or GitHub while gathering context.

## 5. Write the brief

Task id: `<workflow>-<target-slug>` lowercase (`implement-aa4fi-1234`, `review-webapp-42`); if `data/<id>/` already exists, append `-2`, `-3`, ….

Follow the backlog step this home requires before spawning (section 7 of `AGENTS.md`), then scaffold with `bin/fm-brief.sh`:

- ship (`implement`, `address`, `conflicts`): `bin/fm-brief.sh <id> <project> --mode <mode>`, with the delivery mode resolved exactly as section 7 says.
- scout (`plan`, `review`, `description`, `ticket`): `bin/fm-brief.sh <id> <project> --scout`.

Fill `## Captain's intent` with the ask and the gathered context (ticket key, title, and description; PR URL and title; selected feedback; the approved plan; `extra=` text).
Fill `## Firstmate spec` with an `Agent: <name>` line, then `Agent instructions:` followed by the agent's instructions (omit the block when empty), then the workflow's contract:

- **implement**: implement the ticket as described (and follow the approved plan when one is given).
  Open the PR as a **draft** and keep it a draft; do not mark it ready.
  After pushing, append `paused [at=<epoch>]: draft PR <url> held for the captain` to the status file instead of `done:`.
  The full `https://` PR URL must appear verbatim in that line.
- **plan**: produce a concrete implementation plan for review in `report.md`. Inspect the repository as needed, but do not modify files.
- **review**: check out the PR (`gh pr checkout <url> --detach`) in the scratch worktree, inspect the changes, and report actionable findings in `report.md`. Do not modify the code or submit a GitHub review.
- **address**: work on the existing PR's head branch (`gh pr checkout <url>`), address the listed feedback, run the relevant tests, commit, and push to that same head branch.
  Do not open a new PR, do not merge or close the PR, and do not resolve or reply to review threads unless the captain asked.
  After pushing, append `done [at=<epoch>]: pushed to <url>` to the status file.
  Nothing is merged and there is no merge poll.
- **conflicts**: use `gh pr view` to identify the base branch, head branch, and head repository; merge the latest base branch into the checked-out PR branch; resolve every conflict preserving the intent of both sides; run the relevant tests; commit the resolution; push HEAD to the PR's actual head branch.
  Do not merge or close the PR.
  After pushing, append `done [at=<epoch>]: pushed to <url>` to the status file.
  Nothing is merged and there is no merge poll.
- **description**: inspect every commit and the complete diff from the base branch; update the PR body with `gh pr edit --body-file` so it accurately reflects all current changes, preserving useful context and removing stale claims (leave it unchanged if already accurate). Put the final body in `report.md`. Do not modify repository files, create commits, or submit a review.
- **ticket**: explore the feature's repository (and relevant sibling projects) and create a concise, evidence-based Jira ticket under the feature with `acli jira workitem create` (check `--help`), including summary, problem or outcome, codebase evidence, implementation direction, acceptance criteria, and tests. Verify it and put the new key in `report.md`. Do not modify source files or create commits.
- **free**: the captain's `prompt=` is the task.

For `address` and `conflicts`, adjust the ship scaffold's delivery sections for working an existing PR, as `bin/fm-brief.sh` allows: the branch is the PR's head branch, and "PR opened" means "pushed to the existing PR". Record the PR on the task the way the ordinary PR flow does so `fm-pr-check`, `fm-pr-merge`, and teardown track it.

## 6. Spawn

`bin/fm-spawn.sh <id> <project-dir> …` exactly as section 7 of `AGENTS.md` requires, plus the agent's `--harness <harness> --model <model> --effort <effort>` (omit empty values).
If `fm-spawn.sh` refuses the harness, model, or effort, report the refusal to the captain; do not substitute another profile.

## 7. Record the run

Right after a successful spawn:

```sh
vessel/bin/vessel-record-run.sh --task <id> --workflow <workflow> --agent "<name>" \
  --harness <harness> --model <model> --effort <effort> --project <project> --title "<ticket or PR title>" \
  [--ticket <KEY>] [--repo <owner/repo>] [--pr <n>] [--pr-url <url>] [--pr-head <headRefOid>] \
  [--plan-task <task>] [--request-id <inbox note id>]
```

Include `--ticket` whenever a Jira key is known (also for PR workflows whose PR title names one) and `--repo`/`--pr`/`--pr-head` for every PR workflow.
For an inbox request, acknowledge the note and reply with the task id through `bin/fm-inbox.sh` as usual.

Supervision continues as normal after the run record is written.
When the worker reports the PR ready (a `paused:` line for `implement`, a `done:` line for `address` and `conflicts`), follow the Handoff section below instead of the ordinary merge path.

## 8. Handoff

When a ship worker reports its PR - a `paused [at=…]: draft PR <url> held for the captain` for `implement`, or `done [at=…]: pushed to <url>` for `address` and `conflicts` - carry out these steps in order:

1. **Do not run `fm-pr-check.sh`.**
   The merge poll is not armed at handoff; it is armed later, after the captain marks the PR ready (step 6 below).

2. **Record the handoff** by calling:
   ```sh
   vessel/bin/vessel-record-run.sh --update <task-id> --state handed-off \
     --pr-url <url> --pr-head <headRefOid>
   ```
   Read the PR URL and head from the status line or from `gh pr view <url> --json url,headRefOid`.

3. **Stop the worker agent** with `bin/fm-control.sh <task-id> exit`.
   This stops the agent process while keeping the task record and isolated copy alive.
   Do NOT run `bin/fm-teardown.sh` here: the task record must stay alive until the PR is merged or closed, because `bin/fm-pr-merge.sh` requires the task metadata.

4. **Run the configured `pr_open` Jira transition.**
   Look up the transition name at `jira_transitions.pr_open` in `config/vessel/vessel.json` (falling back to `vessel/defaults/vessel.json` when the user file is absent).
   Call `vessel/bin/vessel-jira.sh transition --ticket <ticket-key> --to <transition-name>` when a Jira key is known.

5. **File the "mark ready and request reviewers" proposal** by calling:
   ```sh
   bin/fm-captain-hold.sh hold draft-handoff-<pr-slug> \
     --title "Mark PR ready: <pr-title>" \
     --reason "Mark <url> ready for review and request review from CODEOWNERS or recent reviewers"
   ```
   The proposal id is `draft-handoff-<pr-slug>` where `<pr-slug>` is the PR number extracted from the URL (e.g. `draft-handoff-pr-42`).

6. **When the captain accepts the "mark ready" proposal:**
   - Answer the hold: `bin/fm-captain-hold.sh answer draft-handoff-<pr-slug> --decision-file <path>`.
   - Run `gh pr ready <url> --repo <owner/repo>` to mark the PR ready for review.
   - Request reviewers from CODEOWNERS or recent contributors: `gh pr edit <url> --add-reviewer <reviewer> --repo <owner/repo>`.
   - Arm the merge poll: `bin/fm-pr-check.sh <task-id> <url>` (it refuses drafts, so this only succeeds after step above).
   - For address and conflicts follow-ups on the same PR: reuse the existing task via `bin/fm-control.sh <task-id> relaunch` instead of creating a new task.

7. **When the PR is merged or closed** (merge poll fires or captain reports it):
   - Clean up with `bin/fm-teardown.sh <task-id>`.
   The branch is already pushed to the remote, so teardown accepts it as landed without `--force`.

The vessel TUI shows the run as "in review" (blue `◑`) from this point until the ledger records a merge or the captain answers the proposal.

## 9. Radar loop

**Trigger:** a `check:` notification whose reason line starts with `vessel-radar:`.
This section and nothing else processes `vessel-radar:` notifications.
Do not poll, scan, or read radar state outside this trigger.

### 9.1 Read pending events

```sh
vessel radar pending --json
```

This prints the current pending events as a JSON array.
Each event has `id`, `kind`, `target`, and `anchor` fields.
An empty array means nothing to do; skip to the end of this section.

### 9.2 Read autoprep config

Read `config/vessel/vessel.json` with `jq`, falling back to `vessel/defaults/vessel.json` when the user file is absent.
The relevant keys are `autoprep.max_concurrent` (default 3) and `autoprep.<kind>` (default true for every kind).

Count active preparation scouts: runs in `data/vessel/runs.jsonl` whose `workflow` is `triage` or `diagnose` and that have no matching `handed-off` update record, and whose firstmate task is not yet cleaned up.
Compare against `max_concurrent`.

While the away posture (`state/.afk-contract`) is present, preparation still runs.
Proposals filed during away mode wait in OPEN DECISIONS and are surfaced on the captain's return.

### 9.3 Compute the event slug

For each pending event, compute the `event-slug` used in proposal IDs and task IDs:

- GitHub PR URL (`https://github.com/<owner>/<repo>/pull/<n>`): extract `<repo>-<n>`, e.g. `webapp-42`.
- Jira ticket key (`PROJ-123`): lowercase the key, e.g. `proj-123`.
- Fallback: take the last path segment of the target.

The full event slug is `<kind>-<slug>`, e.g. `ci-failed-webapp-42`.

### 9.4 Check for a supersede

Before processing each event, check whether an open proposal for the same target already exists.
Look for any open captain call whose id matches `proposal-<any-kind>-<slug>` where the slug matches this target.
Use `bin/fm-captain-hold.sh open proposal-<old-kind>-<slug>` to test each candidate.

If an open proposal for the same target exists:
1. Cancel any in-progress preparation scout for the old event: `bin/fm-control.sh <old-scout-task> exit`.
2. Answer the old proposal as superseded:
   ```sh
   printf 'superseded by %s at anchor %s\n' "<new-kind>" "<new-anchor>" > /tmp/supersede-decision.txt
   bin/fm-captain-hold.sh answer proposal-<old-kind>-<slug> --decision-file /tmp/supersede-decision.txt
   ```

### 9.5 Pick the playbook row

| Event kind | Preparation agent mode | Notes |
|---|---|---|
| `review-comment` | Triage | Both `review-comment` and `unresolved-thread` on the same PR may share one Triage run |
| `unresolved-thread` | Triage | See above |
| `review-requested` | Review (Snoop) | Use incremental mode if a previous Snoop report exists for this PR |
| `ci-failed` | Diagnose | |
| `conflicts` | none | Go straight to proposal |
| `approved` | none | Go straight to proposal |
| `draft-handoff` | none | Go straight to proposal (the Handoff section handles this, not the Radar loop) |
| `ticket-assigned` | Plan (Stringer) | |

For `review-requested` in incremental mode, check whether `data/review-<pr-slug>/findings.json` exists from a previous Snoop run.
If it does, include the previous `pr_head` and the previous `report.md` path in the brief's `## Captain's intent`.

### 9.6 Dispatch the preparation scout

If the event type is disabled in autoprep (`autoprep.<kind>` is false) or the `max_concurrent` cap is reached, skip preparation and go directly to proposal (section 9.8) with a note that preparation was skipped.

If preparation is needed:

1. **Resolve the agent** by mode from `config/vessel/agents.json` (falling back to `vessel/defaults/agents.json`) using the same logic as section 2.

2. **Compute the task id:** `<workflow>-<slug>`, e.g. `triage-webapp-42`.
   If `data/<id>/` already exists, append `-2`, `-3`, and so on.

3. **Resolve the project** using section 3's logic.

4. **Write the brief** with `bin/fm-brief.sh <id> <project> --scout`.
   Fill `## Captain's intent` with the event description, the full PR/ticket URL, and any incremental-mode context.
   Fill `## Firstmate spec` with an `Agent: <name>` line, `Agent instructions:` followed by the agent's instructions, and the workflow contract from section 5.

5. **Spawn** with `bin/fm-spawn.sh <id> <project-dir>` plus the agent's `--harness`, `--model`, and `--effort`.

6. **Record the run:**
   ```sh
   vessel/bin/vessel-record-run.sh --task <id> --workflow <workflow> --agent "<name>" \
     --harness <harness> --model <model> --effort <effort> --project <project> \
     --title "<title>" [--repo <owner/repo>] [--pr <n>] [--radar-event <event-id>]
   ```

7. **Ack the event:** `vessel radar ack <event-id>`.

### 9.7 When a preparation scout finishes

When a preparation scout task (workflow `triage`, `diagnose`, `review`, or `plan` with `radar_event` set) reports `done:` via a `signal:` wake:

1. Read `data/<task>/report.md`.
2. For a Triage scout: also read `data/<task>/triage.json` (schema: `vessel/docs/prep-formats.md`).
   Extract the count of items by action type.
3. For a Snoop scout: also read `data/<task>/findings.json` (schema: `vessel/docs/prep-formats.md`).
   Extract the suggested verdict and finding counts.
4. File a proposal (section 9.8).

### 9.8 File a proposal

Run:
```sh
bin/fm-captain-hold.sh hold proposal-<event-slug> \
  --title "<title>" \
  --reason "<recommendation>"
```

Then send one self-contained chat message to the captain containing:
- What happened, with the full URL.
- What the preparation found, briefly (verdict, finding counts, or action breakdown).
- The recommendation.
- The alternatives.
- "Accept?"

Proposal id: `proposal-<event-slug>` where `event-slug` is `<kind>-<slug>` as defined in section 9.3.

**Typical proposals by event kind:**

- `review-comment` / `unresolved-thread`: "Fix N (plan attached), reply to M (draft), push back on P (draft), file Q as follow-up ticket. Accepting also authorizes re-requesting review after the fix is pushed. Accept?"
- `review-requested`: "Request changes: N blocking, M nits (pending review ready). Accept?" or "Approve: all N of your threads addressed. Accept?"
- `ci-failed`: "Flaky test - rerun. Accept?" or "Real failure - fix (plan attached). Accept?" or "Unrelated failure on main. Accept?"
- `conflicts`: "Resolve merge conflicts with Conflict Resolver. Accept?"
- `approved`: "PR is approved and checks are green - ready to merge. Accept?"
- `ticket-assigned`: "Pick up PROJ-123 with this plan (attached). Accept?" or "Ticket is unclear - post these questions (draft attached). Accept?"

### 9.9 Captain's answer

When the captain accepts or redirects a proposal:

1. Write the captain's words to a file.
2. Answer the hold: `bin/fm-captain-hold.sh answer proposal-<event-slug> --decision-file <path>`.
3. Carry out exactly the named actions from the on-accept column below.
4. Release the hold when done.

Accepting counts as the captain's explicit command for exactly the actions named in the recommendation, nothing broader.
A proposal that names "re-request review after fix" explicitly authorizes that re-request when accepted.

**On-accept actions by event kind:**

- `review-comment` / `unresolved-thread`: dispatch Proposition Joe on the accepted items (`address` workflow); post accepted draft replies with `vessel/bin/vessel-reply.sh`; file follow-up tickets with `vessel/bin/vessel-jira.sh create`; re-request review after the fix is pushed.
- `review-requested` / incremental: publish the pending review with accepted findings and submit with the accepted verdict using `vessel/bin/vessel-publish-review.sh`; resolve addressed threads if accepted.
- `ci-failed` (flaky): `gh run rerun --failed <run-id> --repo <repo>`.
- `ci-failed` (real failure): dispatch Slim Charles with the Diagnose report as the plan.
- `ci-failed` (unrelated): no action needed; acknowledge to the captain.
- `conflicts`: dispatch Conflict Resolver (`conflicts` workflow).
- `approved`: merge through `bin/fm-pr-merge.sh <task-id> <url>` using the task id from the recorded run for this PR.
- `ticket-assigned`: dispatch Stringer for a plan (if the captain redirected) or Slim Charles with the plan (if accepted directly), or post the questions with `vessel/bin/vessel-jira.sh comment`.


## 10. Outward mechanics

These scripts run only after the captain's explicit accept or command.
Do not call them in worker briefs or from any automatic path.
Firstmate calls them directly after the captain accepts a proposal.
Each script prints its dry-run plan and requires `--yes` to act.
See the outward-action policy in `vessel/docs/team-plan.md`.

- **`vessel/bin/vessel-publish-review.sh --task <id> [--home <fm-home>] [--findings <id,...>] [--submit COMMENT|REQUEST_CHANGES|APPROVE] [--resolve-threads <id,...>] [--yes]`**:
  Submit a pending GitHub review built from `data/<task>/findings.json`.
  Applies captain edits from `data/vessel/review-edits/<task>.json` when present.
  Refuses when the PR head moved since the review, or a pending review already exists.
- **`vessel/bin/vessel-reply.sh --task <id> [--home <fm-home>] [--items <id,...>] [--no-rerequest] [--yes]`**:
  Post draft replies from `data/<task>/triage.json` to PR threads and re-request review.
- **`vessel/bin/vessel-jira.sh pickup|transition|comment|create --ticket <KEY> [--to <name>] [--body <text>] [--summary <text>] [--home <fm-home>] [--yes]`**:
  Wrap `acli jira workitem` using transition names from `jira_transitions` in `config/vessel/vessel.json`.
  Default transitions: `pickup` → `"In Progress"`, `pr_open` → `"In Review"`.

File format reference for findings.json, triage.json, and review-edits: `vessel/docs/prep-formats.md`.

## 11. Standup digest

**Trigger:** the captain asks for a standup, a daily digest, or "what did I do yesterday".
This section and nothing else handles that request.
Do not dispatch a scout or any other worker for it.

Run `vessel standup [--since <YYYY-MM-DD|yesterday>]` in the firstmate checkout (it resolves the home the usual way) and relay its markdown as the answer.
With no date in the request, run it without `--since` so it covers the prior day.
The command is read-only: it never dispatches work and never writes to GitHub or Jira.
When the captain asks for a different window, pass it through `--since` instead of filtering the output by hand.
