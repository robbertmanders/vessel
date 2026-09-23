//! Regression coverage for the vessel-reply.sh item filter.
//!
//! Reply publishing used to fail jq compilation on every run because the
//! --items filter expression used the invalid `..id` syntax, and the invalid
//! expression was compiled even on the unfiltered path. These tests drive the
//! script's dry run (no --yes, so no live GitHub writes) with a fixture
//! triage.json covering the unfiltered, single-item, and multi-item cases.

use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

fn firstmate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixture_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vessel-reply-filter-{}-{}",
        tag,
        std::process::id()
    ));
    let task_dir = dir.join("data").join("filter-task");
    fs::create_dir_all(&task_dir).unwrap();
    fs::write(
        task_dir.join("triage.json"),
        r#"{"pr_url":"https://github.com/owner/repo/pull/42","items":[
{"id":"t1","thread_id":null,"comment_id":1,"action":"reply","summary":"one","draft_reply":"hello one"},
{"id":"t2","thread_id":null,"comment_id":2,"action":"pushback","summary":"two","draft_reply":"hello two"},
{"id":"t3","thread_id":null,"comment_id":3,"action":"fix","summary":"three","draft_reply":"hello three"},
{"id":"t4","thread_id":null,"comment_id":4,"action":"reply","summary":"four","draft_reply":""}
]}"#,
    )
    .unwrap();
    // The script requires a gh binary on PATH but the dry run never calls it.
    let bin_dir = dir.join("stub-bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let stub = bin_dir.join("gh");
    fs::write(&stub, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

fn dry_run(home: &PathBuf, extra_args: &[&str]) -> String {
    let path = format!(
        "{}:{}",
        home.join("stub-bin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut args = vec!["--task", "filter-task", "--home"];
    let home_str = home.to_string_lossy().to_string();
    args.push(&home_str);
    args.extend_from_slice(extra_args);
    let output = Command::new(firstmate_root().join("vessel/bin/vessel-reply.sh"))
        .args(&args)
        .env("PATH", path)
        .env("FM_HOME", home)
        .output()
        .expect("vessel-reply.sh should run");
    assert!(
        output.status.success(),
        "dry run failed (jq compile error?): {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn unfiltered_dry_run_lists_reply_and_pushback_items() {
    let home = fixture_home("unfiltered");
    let out = dry_run(&home, &[]);
    assert!(out.contains("replies: 2"), "unexpected output:\n{out}");
    assert!(out.contains("[reply t1]"), "unexpected output:\n{out}");
    assert!(out.contains("[pushback t2]"), "unexpected output:\n{out}");
    fs::remove_dir_all(&home).ok();
}

#[test]
fn single_item_filter_selects_one_item() {
    let home = fixture_home("single");
    let out = dry_run(&home, &["--items", "t2"]);
    assert!(out.contains("replies: 1"), "unexpected output:\n{out}");
    assert!(out.contains("[pushback t2]"), "unexpected output:\n{out}");
    assert!(!out.contains("[reply t1]"), "unexpected output:\n{out}");
    fs::remove_dir_all(&home).ok();
}

#[test]
fn multi_item_filter_selects_matching_replyable_items() {
    // t3 is action=fix so only t1 of the two named items has a reply.
    let home = fixture_home("multi");
    let out = dry_run(&home, &["--items", "t1,t3"]);
    assert!(out.contains("replies: 1"), "unexpected output:\n{out}");
    assert!(out.contains("[reply t1]"), "unexpected output:\n{out}");
    assert!(!out.contains("[pushback t2]"), "unexpected output:\n{out}");
    fs::remove_dir_all(&home).ok();
}
