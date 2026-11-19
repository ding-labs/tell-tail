use std::{fs, process::Command};
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tell-tail"))
}
#[test]
fn desktop_once_unifies_observations_without_private_values() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("desktop.log"),
        include_str!("fixtures/desktop-activity.log"),
    )
    .unwrap();
    let result = bin()
        .args(["desktop", "--source", "codex-desktop", "--log-dir"])
        .arg(dir.path())
        .args(["--once", "--output", "json"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(!text.contains("PRIVATE"));
    let values: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let items: Vec<_> = values
        .iter()
        .filter(|v| v["kind"] == "activity_item")
        .collect();
    assert_eq!(items.len(), 14);
    assert!(items.iter().any(|v| v["item"]["category"] == "mcp"));
    assert!(items.iter().any(|v| v["item"]["category"] == "connections"));
    assert_eq!(
        values.last().unwrap()["snapshot"]["sources"]["running"],
        false
    );
}
#[test]
fn desktop_help_and_literal_filename_are_compatible() {
    let help = bin().args(["desktop", "--help"]).output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("tell-tail desktop"));
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("desktop"), "INFO example\n").unwrap();
    for args in [
        vec!["./desktop", "--once", "--output", "json"],
        vec!["--once", "--output", "json", "--", "desktop"],
    ] {
        assert!(
            bin()
                .current_dir(dir.path())
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[test]
fn desktop_demo_is_isolated_and_includes_every_category() {
    let dir = tempfile::tempdir().unwrap();
    let output = bin()
        .env("HOME", dir.path())
        .env_remove("CODEX_HOME")
        .args(["desktop", "--demo", "--once", "--output", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    for category in ["tools", "mcp", "connections", "desktop"] {
        assert!(
            rows.iter().any(|v| v["item"]["category"] == category),
            "{category}"
        );
    }
    assert_eq!(rows.last().unwrap()["snapshot"]["sources"]["demo"], true);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
