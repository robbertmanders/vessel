//! `vessel standup [--since <date>]` - a markdown digest for the captain.
//!
//! Read-only: it renders what the fleet already recorded (the fleet ledger,
//! run records, the fleet snapshot, and the captain's own pull requests) and
//! never dispatches work or touches GitHub or Jira beyond those reads.
//! `--since` accepts `YYYY-MM-DD` or `yesterday`; the default is the prior
//! day (yesterday at local midnight).

use std::{
    io::{self},
    path::PathBuf,
};

use chrono::{Local, NaiveDate};

use crate::{
    config::{load_config, resolve_fm_home},
    firstmate::{
        ledger::Ledger,
        runs::{Run, RunStatus, build_runs, load_run_records, load_update_records},
        snapshot::Snapshot,
    },
    github::{ReviewStatus, load_pull_requests},
};

const USAGE: &str = "usage: vessel standup [--since <YYYY-MM-DD|yesterday>] [--home <path>]

Print a markdown digest covering what finished since the given date,
open proposals, runs under way, and your PRs waiting on others.
The default window starts at local midnight yesterday. Read-only:
it never dispatches work and never writes to GitHub or Jira.";

pub(crate) fn run(args: &[String]) -> io::Result<()> {
    let mut home = None;
    let mut since_raw: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--home" => home = iter.next().map(PathBuf::from),
            "--since" => since_raw = iter.next().cloned(),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => {
                eprintln!("vessel standup: unknown argument {other}\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let (since_ts, since_label) = match since_raw.as_deref().map(parse_since) {
        None => default_since(),
        Some(Ok(parsed)) => parsed,
        Some(Err(message)) => {
            eprintln!("vessel standup: {message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let (fm_home, fm_root) = resolve_fm_home(home).map_err(io::Error::other)?;
    let config = load_config((fm_home.clone(), fm_root.clone())).map_err(io::Error::other)?;
    if let Some(jql) = &config.jira_jql {
        crate::jira::set_jql(jql.clone());
    }

    let snapshot =
        crate::firstmate::snapshot::load_snapshot(&fm_root, &fm_home).map_err(io::Error::other)?;
    let runs_file = config.runs_file();
    let records = load_run_records(&runs_file).unwrap_or_default();
    let updates = load_update_records(&runs_file).unwrap_or_default();
    let mut ledger = Ledger::new(config.ledger_file());
    ledger.poll().unwrap_or_default();
    let runs = build_runs(&records, &updates, &ledger, Some(&snapshot));

    let pull_requests = if config.github_enabled {
        match load_pull_requests() {
            Ok(prs) => Some(Ok(prs)),
            Err(error) => Some(Err(error)),
        }
    } else {
        None
    };

    print!(
        "{}",
        render(
            &snapshot,
            &runs,
            pull_requests.as_ref(),
            since_ts,
            &since_label
        )
    );
    Ok(())
}

fn default_since() -> (u64, String) {
    let today = Local::now().date_naive();
    let yesterday = today.pred_opt().unwrap_or(today);
    let label = yesterday.format("%Y-%m-%d").to_string();
    let midnight = yesterday.and_hms_opt(0, 0, 0).unwrap_or_else(|| {
        today
            .and_hms_opt(0, 0, 0)
            .expect("midnight exists for any calendar day")
    });
    let ts = midnight
        .and_local_timezone(Local)
        .single()
        .map(|zoned| zoned.timestamp() as u64)
        .unwrap_or(0);
    (ts, label)
}

fn parse_since(raw: &str) -> Result<(u64, String), String> {
    if raw == "yesterday" {
        return Ok(default_since());
    }
    let date = NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| format!("--since must be YYYY-MM-DD or 'yesterday', got {raw:?}"))?;
    let label = date.format("%Y-%m-%d").to_string();
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| format!("--since date {raw:?} has no midnight"))?;
    let ts = midnight
        .and_local_timezone(Local)
        .single()
        .map(|zoned| zoned.timestamp() as u64)
        .ok_or_else(|| format!("--since date {raw:?} is ambiguous in local time"))?;
    Ok((ts, label))
}

/// Single-line text for a markdown list item: no newlines, bounded length.
fn one_line(text: &str) -> String {
    const LIMIT: usize = 160;
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.len() <= LIMIT {
        return flat;
    }
    let mut end = LIMIT;
    while !flat.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", flat[..end].trim_end())
}

fn status_label(status: RunStatus) -> &'static str {
    status.label()
}

fn render(
    snapshot: &Snapshot,
    runs: &[Run],
    pull_requests: Option<&Result<Vec<crate::github::PullRequest>, String>>,
    since_ts: u64,
    since_label: &str,
) -> String {
    let mut out = format!("# Standup (since {since_label})\n\n");

    out.push_str("## Did\n\n");
    let mut finished: Vec<&Run> = runs
        .iter()
        .filter(|run| run.finished_at.is_some_and(|at| at >= since_ts))
        .collect();
    finished.sort_by(|left, right| {
        left.finished_at
            .cmp(&right.finished_at)
            .then_with(|| left.task.cmp(&right.task))
    });
    if finished.is_empty() {
        out.push_str("- None.\n");
    }
    for run in finished {
        let mut item = format!(
            "- {} ({}, {})",
            run.title,
            run.task,
            status_label(run.status)
        );
        if let Some(url) = run.pr_url.as_deref() {
            item.push_str(&format!(" - {url}"));
        } else if let Some(ticket) = run.ticket_key.as_deref() {
            item.push_str(&format!(" - {ticket}"));
        }
        out.push_str(&one_line(&item));
        out.push('\n');
    }

    out.push_str("\n## Open proposals\n\n");
    let mut proposals: Vec<_> = snapshot
        .backlog
        .iter()
        .filter(|record| record.captain_actionable)
        .collect();
    proposals.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.title.cmp(&right.title))
    });
    if proposals.is_empty() {
        out.push_str("- None.\n");
    }
    for proposal in proposals {
        let id = proposal.id.as_deref().unwrap_or("unfiled");
        let reason = proposal
            .hold_reason
            .as_deref()
            .unwrap_or("waiting on the captain");
        out.push_str(&one_line(&format!("- {} ({id}): {reason}", proposal.title)));
        out.push('\n');
    }

    out.push_str("\n## Runs under way\n\n");
    let mut active: Vec<&Run> = runs.iter().filter(|run| run.is_active()).collect();
    active.sort_by(|left, right| left.task.cmp(&right.task));
    if active.is_empty() {
        out.push_str("- None.\n");
    }
    for run in active {
        let mut item = format!(
            "- {} ({}, {})",
            run.title,
            run.task,
            status_label(run.status)
        );
        if let Some(detail) = run.status_text.as_deref().filter(|text| !text.is_empty()) {
            item.push_str(&format!(" - {detail}"));
        }
        out.push_str(&one_line(&item));
        out.push('\n');
    }

    out.push_str("\n## My PRs waiting on others\n\n");
    match pull_requests {
        None => out.push_str("- GitHub tracking is off.\n"),
        Some(Err(error)) => {
            let first = error.lines().next().unwrap_or("unknown error");
            out.push_str(&one_line(&format!("- GitHub unavailable ({first}).")));
            out.push('\n');
        }
        Some(Ok(prs)) => {
            // `needs_attention` marks drafts and change-requests: work the
            // captain owes. Everything else waits on someone else.
            let mut waiting: Vec<_> = prs.iter().filter(|pr| !pr.needs_attention).collect();
            waiting.sort_by(|left, right| {
                left.repository
                    .cmp(&right.repository)
                    .then_with(|| left.number.cmp(&right.number))
            });
            if waiting.is_empty() {
                out.push_str("- None.\n");
            }
            for pr in waiting {
                let review = match pr.status {
                    ReviewStatus::Draft => "draft",
                    ReviewStatus::Waiting => "waiting for review",
                    ReviewStatus::ChangesRequested => "changes requested",
                    ReviewStatus::Approved => "approved",
                };
                let ci = pr.ci_status.as_deref().unwrap_or("CI unknown");
                out.push_str(&one_line(&format!(
                    "- {}#{} {} ({review}, {ci}) {}",
                    pr.repository, pr.number, pr.title, pr.url
                )));
                out.push('\n');
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firstmate::snapshot::{BacklogRecord, Snapshot};

    #[test]
    fn since_accepts_a_date_and_yesterday() {
        let (ts, label) = parse_since("2026-09-22").expect("YYYY-MM-DD should parse");
        assert_eq!(label, "2026-09-22");
        assert!(ts > 0);
        assert!(parse_since("22-09-2026").is_err());
        assert!(parse_since("tomorrow").is_err());
        let (_, yesterday_label) = parse_since("yesterday").expect("yesterday parses");
        assert_eq!(yesterday_label, default_since().1);
    }

    #[test]
    fn empty_sources_render_none_everywhere() {
        let snapshot = Snapshot::default();
        let text = render(&snapshot, &[], None, 0, "2026-09-22");
        for section in [
            "## Did",
            "## Open proposals",
            "## Runs under way",
            "## My PRs waiting on others",
        ] {
            assert!(text.contains(section), "missing {section}");
        }
        assert_eq!(text.matches("- None.").count(), 3);
        assert!(text.contains("GitHub tracking is off."));
    }

    #[test]
    fn sections_are_sorted_and_deterministic() {
        let snapshot = Snapshot {
            backlog: vec![
                BacklogRecord {
                    state: "held".into(),
                    id: Some("proposal-b".into()),
                    title: "Second proposal".into(),
                    hold_reason: Some(" quests ".into()),
                    captain_actionable: true,
                    ..BacklogRecord::default()
                },
                BacklogRecord {
                    state: "held".into(),
                    id: Some("proposal-a".into()),
                    title: "First proposal".into(),
                    hold_reason: None,
                    captain_actionable: true,
                    ..BacklogRecord::default()
                },
                BacklogRecord {
                    state: "queued".into(),
                    id: Some("not-actionable".into()),
                    title: "Not a proposal".into(),
                    hold_reason: Some("later".into()),
                    captain_actionable: false,
                    ..BacklogRecord::default()
                },
            ],
            ..Snapshot::default()
        };
        let text = render(&snapshot, &[], None, 0, "2026-09-22");
        let a = text.find("proposal-a").expect("proposal-a renders");
        let b = text.find("proposal-b").expect("proposal-b renders");
        assert!(a < b, "proposals sort by id");
        assert!(
            !text.contains("not-actionable"),
            "non-actionable rows stay out"
        );
        assert!(
            text.contains("waiting on the captain"),
            "missing reason has a fallback"
        );
    }
}
