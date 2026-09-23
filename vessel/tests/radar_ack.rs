//! Regression coverage for the Radar ack ordering the skill's Radar loop depends on.
//!
//! Skipped-preparation events are acked only after their proposal is durably
//! filed, so `ack` must remove exactly the filed event while every unacked
//! event stays pending for the next loop. These tests drive the built binary
//! against a fixture home through the public `radar pending` / `radar ack`
//! interface.

use std::{fs, path::PathBuf, process::Command};

fn vessel_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_vessel"))
}

fn fixture_home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "vessel-radar-ack-{}-{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(home.join("state")).unwrap();
    fs::create_dir_all(home.join("data/vessel")).unwrap();
    home
}

fn seed_pending(home: &PathBuf, ids: &[&str]) {
    let pending: Vec<serde_json::Value> = ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "target": "https://github.com/acme/webapp/pull/42",
                "kind": "conflicts",
                "anchor": "abc123",
            })
        })
        .collect();
    let state = serde_json::json!({ "v": 1, "seen": [], "pending": pending });
    fs::write(
        home.join("data/vessel/radar.json"),
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();
}

fn radar(home: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(vessel_bin())
        .arg("radar")
        .args(args)
        .arg("--home")
        .arg(home)
        .env_remove("VESSEL_FM_HOME")
        .env_remove("FM_HOME")
        .output()
        .expect("vessel binary runs")
}

fn read_state(home: &PathBuf) -> serde_json::Value {
    let text = fs::read_to_string(home.join("data/vessel/radar.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn ack_removes_only_the_filed_event_and_leaves_the_rest_pending() {
    let home = fixture_home("filed");
    seed_pending(&home, &["event-filed", "event-unfiled"]);

    // The filed proposal's event is acked after the hold succeeds.
    let acked = radar(&home, &["ack", "event-filed"]);
    assert!(acked.status.success());

    // The public pending listing still shows the unfiled event for the next loop.
    let pending = radar(&home, &["pending", "--json"]);
    assert!(pending.status.success());
    let listed: serde_json::Value =
        serde_json::from_str(String::from_utf8(pending.stdout).unwrap().trim()).unwrap();
    let ids: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["event-unfiled"]);

    // The filed event moved to seen so it is never reprocessed.
    let state = read_state(&home);
    assert_eq!(state["pending"].as_array().unwrap().len(), 1);
    let seen: Vec<&str> = state["seen"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(seen, vec!["event-filed"]);

    fs::remove_dir_all(&home).ok();
}

#[test]
fn failed_or_skipped_ack_leaves_everything_pending() {
    let home = fixture_home("unfiled");
    seed_pending(&home, &["event-a", "event-b"]);

    // Acking an unknown id (or running with no ids) must not drop real events.
    let unknown = radar(&home, &["ack", "event-missing"]);
    assert!(unknown.status.success());
    let empty = radar(&home, &["ack"]);
    assert!(empty.status.success());

    let pending = radar(&home, &["pending", "--json"]);
    let listed: serde_json::Value =
        serde_json::from_str(String::from_utf8(pending.stdout).unwrap().trim()).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 2);
    assert!(read_state(&home)["seen"].as_array().unwrap().is_empty());

    fs::remove_dir_all(&home).ok();
}
