use assert_cmd::Command;

#[test]
fn help_lists_setup_subcommand() {
    let assert = Command::cargo_bin("mac-k3d")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.contains("setup"), "help should list setup:\n{stdout}");
    assert!(stdout.contains("prepare"));
    assert!(stdout.contains("eval"), "help should list eval:\n{stdout}");
}

#[test]
fn setup_help_describes_wizard() {
    Command::cargo_bin("mac-k3d")
        .unwrap()
        .args(["setup", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("wizard"));
}

#[test]
fn eval_help_mentions_stage() {
    Command::cargo_bin("mac-k3d")
        .unwrap()
        .args(["eval", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("stage"));
}

#[test]
fn eval_help_mentions_model() {
    Command::cargo_bin("mac-k3d")
        .unwrap()
        .args(["eval", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("model"));
}

#[test]
fn no_subcommand_without_tty_exits_2() {
    Command::cargo_bin("mac-k3d")
        .unwrap()
        .write_stdin("")
        .assert()
        .code(2)
        .stderr(predicates::str::contains("not a TTY"));
}

#[test]
fn start_help_mentions_cluster() {
    Command::cargo_bin("mac-k3d")
        .unwrap()
        .args(["start", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("k3d"));
}
