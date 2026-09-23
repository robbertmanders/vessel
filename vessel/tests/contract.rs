//! Contract checks against the firstmate checkout vessel lives in.
//!
//! vessel never edits firstmate's files; it relies on these documented
//! interfaces instead. The sync workflow runs these after merging upstream, so
//! an upstream change that breaks one stops the merge from reaching `main`.

use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

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
        // Phase 4 outward scripts referenced in section 9.
        "vessel/bin/vessel-publish-review.sh",
        "vessel/bin/vessel-reply.sh",
        "vessel/bin/vessel-jira.sh",
        "vessel/docs/prep-formats.md",
    ] {
        assert!(skill.contains(path), "the skill no longer names {path}");
        assert!(firstmate_root().join(path).exists(), "{path} is missing");
    }
}

#[test]
fn outward_scripts_refuse_without_yes() {
    // Each outward script requires --yes to act; without it only a dry run is printed.
    // Missing a required argument (--task) exits with code 2.
    let scripts = [
        "vessel/bin/vessel-publish-review.sh",
        "vessel/bin/vessel-reply.sh",
    ];
    for script in scripts {
        let status = std::process::Command::new("bash")
            .args([
                "-c",
                &format!(
                    "'{}/{}' --task nonexistent-task --home /nonexistent 2>/dev/null; echo $?",
                    firstmate_root().display(),
                    script
                ),
            ])
            .output()
            .expect("bash should be available");
        // The script should exit non-zero (missing findings/triage file).
        let output = String::from_utf8_lossy(&status.stdout);
        let exit_code: u32 = output.trim().parse().unwrap_or(0);
        assert!(
            exit_code != 0,
            "{script} should exit non-zero when the required file is missing"
        );
    }

    // vessel-jira.sh with no subcommand exits 2.
    let jira_status =
        std::process::Command::new(firstmate_root().join("vessel/bin/vessel-jira.sh"))
            .stderr(std::process::Stdio::null())
            .status()
            .expect("vessel-jira.sh should be executable");
    assert!(
        !jira_status.success(),
        "vessel-jira.sh should fail with no subcommand"
    );
}

#[test]
fn outward_scripts_print_dry_run_without_yes() {
    // vessel-jira.sh pickup without --yes prints the dry-run plan and exits 0.
    // Pipe a nonexistent FM_HOME so it reads config/vessel/vessel.json as absent
    // and falls back to defaults.
    let output = std::process::Command::new(firstmate_root().join("vessel/bin/vessel-jira.sh"))
        .args(["pickup", "--ticket", "TEST-1", "--home", "/nonexistent"])
        .env("FM_HOME", "/nonexistent")
        .output()
        .expect("vessel-jira.sh should be executable");
    // Without --yes it exits 0 (dry run).
    assert!(
        output.status.success(),
        "vessel-jira.sh pickup without --yes should succeed (dry-run exit 0)"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Dry run") || stdout.contains("dry run"),
        "vessel-jira.sh pickup without --yes should print dry-run notice"
    );
    assert!(
        stdout.contains("In Progress"),
        "vessel-jira.sh pickup should show the default transition name"
    );
}

#[test]
fn check_register_accepts_vessel_radar_id_and_creates_trust_binding() {
    let state_dir = std::env::temp_dir().join(format!("vessel-reg-{}", std::process::id()));
    fs::create_dir_all(&state_dir).unwrap();

    let check_sh = state_dir.join("vessel-radar.check.sh");
    fs::write(&check_sh, "#!/usr/bin/env bash\n").unwrap();
    fs::set_permissions(&check_sh, fs::Permissions::from_mode(0o700)).unwrap();

    let status = std::process::Command::new(firstmate_root().join("bin/fm-check-register.sh"))
        .arg("vessel-radar")
        .env("FM_STATE_OVERRIDE", &state_dir)
        .status()
        .expect("fm-check-register.sh should be executable");
    assert!(status.success(), "fm-check-register.sh should succeed");
    assert!(
        state_dir.join("vessel-radar.check-trust").exists(),
        "fm-check-register.sh should create the trust binding"
    );

    let status = std::process::Command::new(firstmate_root().join("bin/fm-check-unregister.sh"))
        .arg("vessel-radar")
        .env("FM_STATE_OVERRIDE", &state_dir)
        .status()
        .expect("fm-check-unregister.sh should be executable");
    assert!(status.success(), "fm-check-unregister.sh should succeed");
    assert!(
        !state_dir.join("vessel-radar.check.sh").exists(),
        "fm-check-unregister.sh should remove check.sh"
    );
    assert!(
        !state_dir.join("vessel-radar.check-trust").exists(),
        "fm-check-unregister.sh should remove the trust binding"
    );

    fs::remove_dir_all(&state_dir).ok();
}

#[test]
fn registered_check_keeps_supervision_running() {
    let state_dir = std::env::temp_dir().join(format!("vessel-sup-{}", std::process::id()));
    fs::create_dir_all(&state_dir).unwrap();

    let check_sh = state_dir.join("vessel-radar.check.sh");
    let check_trust = state_dir.join("vessel-radar.check-trust");
    fs::write(&check_sh, "#!/usr/bin/env bash\n").unwrap();
    fs::set_permissions(&check_sh, fs::Permissions::from_mode(0o700)).unwrap();
    // The trust binding content is validated by the watcher at execution time,
    // not by fm_supervision_status; presence of both files is the supervision gate.
    fs::write(&check_trust, "fm-custom-check-v1\nhash\n").unwrap();

    let lib = firstmate_root().join("bin/fm-supervision-lib.sh");
    let script = format!(
        ". '{}' && fm_supervision_status '{}' && [ \"$FM_SUP_NEEDED\" = true ]",
        lib.display(),
        state_dir.display()
    );
    let status = std::process::Command::new("bash")
        .args(["-c", &script])
        .status()
        .expect("bash should be available");

    fs::remove_dir_all(&state_dir).ok();
    assert!(
        status.success(),
        "FM_SUP_NEEDED should be true when vessel-radar is registered"
    );
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
fn captain_hold_script_accepts_hold_and_answer_flags() {
    // vessel-workflows relies on fm-captain-hold.sh hold and answer.
    // Verify the script exists, is executable, and exposes the required
    // subcommands and flags in its dispatch and header.
    let script_path = firstmate_root().join("bin/fm-captain-hold.sh");
    assert!(script_path.is_file(), "bin/fm-captain-hold.sh is missing");
    let perms = std::fs::metadata(&script_path).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    assert!(
        perms.mode() & 0o111 != 0,
        "bin/fm-captain-hold.sh is not executable"
    );

    let source = read("bin/fm-captain-hold.sh");
    // The dispatch at the end of the script must route both subcommands.
    assert!(
        source.contains("hold) shift; command_hold"),
        "bin/fm-captain-hold.sh no longer dispatches the 'hold' subcommand"
    );
    assert!(
        source.contains("answer) shift; command_answer"),
        "bin/fm-captain-hold.sh no longer dispatches the 'answer' subcommand"
    );
    // The hold subcommand must accept --reason and --title.
    assert!(
        source.contains("--reason"),
        "bin/fm-captain-hold.sh hold no longer accepts --reason"
    );
    assert!(
        source.contains("--title"),
        "bin/fm-captain-hold.sh hold no longer accepts --title"
    );
    // The answer subcommand must accept --decision-file.
    assert!(
        source.contains("--decision-file"),
        "bin/fm-captain-hold.sh answer no longer accepts --decision-file"
    );
    // The open subcommand is used to check whether a proposal is still open.
    assert!(
        source.contains("open) shift; command_open"),
        "bin/fm-captain-hold.sh no longer dispatches the 'open' subcommand"
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

fn vessel_binary() -> PathBuf {
    option_env!("CARGO_BIN_EXE_vessel")
        .map(PathBuf::from)
        .unwrap_or_else(|| firstmate_root().join("vessel/target/debug/vessel"))
}

fn standup_demo_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vessel-standup-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let status = std::process::Command::new(firstmate_root().join("vessel/tests/demo-home.sh"))
        .arg(&dir)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("demo-home.sh should be executable");
    assert!(status.success(), "demo-home.sh should build a fixture home");
    dir
}

fn run_standup(home: &std::path::Path, extra: &[&str]) -> std::process::Output {
    let home = home.to_string_lossy().into_owned();
    let mut args = vec!["standup", "--home", &home];
    args.extend(extra);
    std::process::Command::new(vessel_binary())
        .args(&args)
        .output()
        .expect("vessel standup should run")
}

#[test]
fn standup_help_names_the_since_window() {
    // The new Phase 5 surface: `vessel standup [--since]` and its default.
    let output = std::process::Command::new(vessel_binary())
        .args(["standup", "--help"])
        .output()
        .expect("vessel standup --help should run");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("--since"),
        "standup --help should name --since"
    );
    assert!(
        help.contains("yesterday"),
        "standup --help should document the prior-day default"
    );
}

#[test]
fn standup_reports_the_demo_fleet_in_four_sections() {
    let home = standup_demo_home("fleet");
    let output = run_standup(&home, &["--since", "2020-01-01"]);
    assert!(
        output.status.success(),
        "standup should succeed: {output:?}"
    );
    let digest = String::from_utf8_lossy(&output.stdout);
    for section in [
        "## Did",
        "## Open proposals",
        "## Runs under way",
        "## My PRs waiting on others",
    ] {
        assert!(digest.contains(section), "standup is missing {section}");
    }
    assert!(
        digest.contains("fix-flaky-ci"),
        "Did should list merged work"
    );
    assert!(
        digest.contains("implement-aa4fi-1234"),
        "Runs under way should list the live implement run"
    );
    assert!(
        digest.contains("review-webapp-42"),
        "Runs under way should list the live review run"
    );
    // The demo home disables GitHub, so the PR section must say so plainly.
    assert!(digest.contains("GitHub tracking is off."));
    // The demo home holds nothing for the captain.
    let proposals = digest
        .split("## Open proposals")
        .nth(1)
        .expect("proposals section");
    let proposals = proposals.split("## Runs under way").next().unwrap_or("");
    assert!(
        proposals.contains("- None."),
        "empty proposals should render None"
    );
    fs::remove_dir_all(&home).ok();
}

#[test]
fn standup_window_filters_finished_work_and_rejects_bad_dates() {
    let home = standup_demo_home("window");
    let output = run_standup(&home, &["--since", "2099-01-01"]);
    assert!(
        output.status.success(),
        "standup should succeed: {output:?}"
    );
    let digest = String::from_utf8_lossy(&output.stdout);
    let did = digest.split("## Did").nth(1).expect("Did section");
    let did = did.split("## Open proposals").next().unwrap_or("");
    assert!(
        did.contains("- None."),
        "a future window should show no finished work"
    );
    assert!(
        !did.contains("fix-flaky-ci"),
        "a future window should exclude old finished work"
    );

    let bad = run_standup(&home, &["--since", "someday"]);
    assert!(
        !bad.status.success(),
        "standup should refuse an unparseable --since"
    );
    fs::remove_dir_all(&home).ok();
}
