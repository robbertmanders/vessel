//! Contract checks against the firstmate checkout vessel lives in.
//!
//! vessel never edits firstmate's files; it relies on these documented
//! interfaces instead. The sync workflow runs these after merging upstream, so
//! an upstream change that breaks one stops the merge from reaching `main`.

use std::{fs, path::PathBuf};

fn firstmate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(path: &str) -> String {
    fs::read_to_string(firstmate_root().join(path))
        .unwrap_or_else(|error| panic!("firstmate no longer has {path}: {error}"))
}

#[test]
fn fleet_ledger_contract_is_v1_with_the_four_events() {
    let contract = read("docs/fleet-ledger.md");
    for needle in [
        "config/fleet-ledger",
        "state/fleet-ledger.jsonl",
        "Readers must ignore members and events they do not recognize",
        "`task.dispatched`",
        "`task.status`",
        "`task.merged`",
        "`task.cleaned_up`",
        r#""v":1"#,
    ] {
        assert!(
            contract.contains(needle),
            "docs/fleet-ledger.md lost {needle}"
        );
    }
}

#[test]
fn snapshot_script_still_emits_schema_v1() {
    let script = read("bin/fm-fleet-snapshot.sh");
    assert!(script.contains("fm-fleet-snapshot.v1"));
}

#[test]
fn dispatch_scripts_keep_the_flags_the_skill_uses() {
    let spawn = read("bin/fm-spawn.sh");
    for flag in [
        "--harness",
        "--model",
        "--effort",
        "--scout",
        "--mode",
        "--yolo",
    ] {
        assert!(spawn.contains(flag), "fm-spawn.sh lost {flag}");
    }
    let brief = read("bin/fm-brief.sh");
    for needle in [
        "--scout",
        "--mode",
        "## Captain's intent",
        "## Firstmate spec",
    ] {
        assert!(brief.contains(needle), "fm-brief.sh lost {needle}");
    }
    let inbox = read("bin/fm-inbox.sh");
    assert!(
        inbox.contains("note [--request-id <id>]"),
        "fm-inbox.sh note --request-id is the future hotkey transport"
    );
    let peek = read("bin/fm-peek.sh");
    assert!(peek.contains("exact task id"));
    let guard = read("bin/fm-guard.sh");
    assert!(
        guard.contains("FM_GUARD_READ_ONLY"),
        "vessel peeks in guard read-only mode"
    );
}

#[test]
fn default_agents_use_verified_harness_adapters() {
    let agents: serde_json::Value =
        serde_json::from_str(&read("vessel/defaults/agents.json")).unwrap();
    for agent in agents["agents"].as_array().unwrap() {
        let harness = agent["harness"].as_str().unwrap();
        let reference = firstmate_root()
            .join(".agents/skills/harness-adapters/references/harness")
            .join(format!("{harness}.md"));
        assert!(
            reference.is_file(),
            "{} uses harness {harness}, which firstmate no longer verifies",
            agent["name"]
        );
    }
}

#[test]
fn record_script_writes_a_valid_run_record() {
    let runs = std::env::temp_dir().join(format!("vessel-record-{}.jsonl", std::process::id()));
    let _ = fs::remove_file(&runs);
    let status =
        std::process::Command::new(firstmate_root().join("vessel/bin/vessel-record-run.sh"))
            .args([
                "--task",
                "review-webapp-42",
                "--workflow",
                "review",
                "--agent",
                "Snoop",
                "--repo",
                "acme/webapp",
                "--pr",
                "42",
                "--pr-head",
                "abc123",
            ])
            .env("VESSEL_RUNS_FILE", &runs)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
    assert!(status.success());
    let record: serde_json::Value =
        serde_json::from_str(fs::read_to_string(&runs).unwrap().trim()).unwrap();
    assert_eq!(record["v"], 1);
    assert_eq!(record["workflow"], "review");
    assert_eq!(record["pr_number"], 42);
    assert_eq!(record["ticket_key"], serde_json::Value::Null);

    let rejected =
        std::process::Command::new(firstmate_root().join("vessel/bin/vessel-record-run.sh"))
            .args(["--task", "x", "--workflow", "deploy"])
            .env("VESSEL_RUNS_FILE", &runs)
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
    assert!(!rejected.success());
    fs::remove_file(runs).ok();
}

#[test]
fn skill_points_at_files_that_exist() {
    let skill = read(".agents/skills/vessel-workflows/SKILL.md");
    for path in [
        "vessel/docs/requests.md",
        "vessel/bin/vessel-record-run.sh",
        "vessel/defaults/agents.json",
        "bin/fm-brief.sh",
        "bin/fm-spawn.sh",
        "bin/fm-inbox.sh",
    ] {
        assert!(skill.contains(path), "the skill no longer names {path}");
        assert!(firstmate_root().join(path).exists(), "{path} is missing");
    }
}

#[test]
fn direct_pr_paused_draft_allowance() {
    // vessel instructs implement workers to report paused: instead of done: for draft PRs.
    // This relies on the direct-PR contract in bin/fm-dod-lib.sh allowing a deliberately
    // held draft when the worker reports paused:.
    let dod = read("bin/fm-dod-lib.sh");
    assert!(
        dod.contains("paused"),
        "bin/fm-dod-lib.sh no longer documents the paused: draft allowance"
    );
    assert!(
        dod.contains("draft"),
        "bin/fm-dod-lib.sh no longer mentions the draft PR case"
    );
    // Verify the paused: verb is defined in the classify lib, which the watcher uses.
    let classify = read("bin/fm-classify-lib.sh");
    assert!(
        classify.contains("FM_CLASSIFY_PAUSED_VERB_DEFAULT"),
        "bin/fm-classify-lib.sh no longer defines FM_CLASSIFY_PAUSED_VERB_DEFAULT"
    );
}

#[test]
fn teardown_accepts_pushed_branch_without_force() {
    // vessel calls bin/fm-teardown.sh after pushing the PR branch, without --force.
    // This relies on teardown treating pushed (remote-reachable) work as landed.
    let teardown = read("bin/fm-teardown.sh");
    assert!(
        teardown.contains("reachable from any remote-tracking branch"),
        "bin/fm-teardown.sh no longer documents the landed-by-remote rule"
    );
}

#[test]
fn paused_status_prefix_is_defined() {
    // vessel relies on paused: being the vocabulary for a deliberate external wait,
    // distinct from blocked:, so the watcher does not wedge-escalate an idle pane.
    let classify = read("bin/fm-classify-lib.sh");
    assert!(
        classify.contains("FM_CLASSIFY_PAUSED_VERB_DEFAULT='paused'"),
        "bin/fm-classify-lib.sh no longer defines the paused: verb"
    );

    // The record script must accept --update with state handed-off.
    let runs_file =
        std::env::temp_dir().join(format!("vessel-update-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&runs_file);
    let status =
        std::process::Command::new(firstmate_root().join("vessel/bin/vessel-record-run.sh"))
            .args([
                "--update",
                "implement-foo-1",
                "--state",
                "handed-off",
                "--pr-url",
                "https://github.com/acme/webapp/pull/9",
                "--pr-head",
                "abc123",
            ])
            .env("VESSEL_RUNS_FILE", &runs_file)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
    assert!(status.success(), "vessel-record-run.sh --update failed");
    let record: serde_json::Value =
        serde_json::from_str(std::fs::read_to_string(&runs_file).unwrap().trim()).unwrap();
    assert_eq!(record["type"], "update");
    assert_eq!(record["state"], "handed-off");
    assert_eq!(record["task"], "implement-foo-1");
    std::fs::remove_file(runs_file).ok();
}
