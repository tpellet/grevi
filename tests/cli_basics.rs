use assert_cmd::Command;

#[test]
fn help_lists_every_verb() {
    let out = Command::cargo_bin("hunch")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    for verb in [
        "pick",
        "why",
        "run",
        "is",
        "capabilities",
        "robot-docs",
        "health",
        "init",
    ] {
        assert!(text.contains(verb), "{verb} missing from --help");
    }
    // Wave 2: `add` (Task 14) and `sort` (Task 15) are both listed.
    assert!(
        text.contains("Stage only") && text.contains("Propose moving"),
        "{text}"
    );
}

#[test]
fn unknown_flag_is_usage_error() {
    Command::cargo_bin("hunch")
        .unwrap()
        .args(["pick", "--nope", "x"])
        .assert()
        .code(2);
}

// An out-of-range threshold is rejected by Config::load for every verb, so this stays a
// usage error for the life of the project (unlike a not-yet-implemented verb).
#[test]
fn json_error_envelope_has_kind_hint_example() {
    let out = Command::cargo_bin("hunch")
        .unwrap()
        .args(["--json", "-t", "2", "is", "x"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["exit_code"], 2);
    assert_eq!(v["error"]["kind"], "usage");
    assert!(v["error"]["example"].as_str().unwrap().contains("hunch"));
}

// Clap fails before any Config exists; agents still get exactly one envelope on stdout.
#[test]
fn clap_usage_error_under_json_is_an_envelope() {
    let out = Command::cargo_bin("hunch")
        .unwrap()
        .args(["--json", "pick", "--nope", "x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["kind"], "usage");
    assert_eq!(v["command"], "pick");
    let out = Command::cargo_bin("hunch")
        .unwrap()
        .args(["pick", "--format", "toon", "--nope", "x"])
        .output()
        .unwrap();
    assert!(String::from_utf8(out.stdout).unwrap().contains("ok: false"));
}

#[test]
fn toon_format_renders() {
    let out = Command::cargo_bin("hunch")
        .unwrap()
        .args(["--format", "toon", "-t", "2", "is", "x"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("ok: false"), "{s}");
}

// `trailing_var_arg` would swallow these into the intent (clap_builder Arg::trailing_var_arg docs).
#[test]
fn run_flags_after_intent_are_flags() {
    use clap::{CommandFactory, FromArgMatches};
    // In-process parse: clap would read `env = "HUNCH_THRESHOLD"` from this test's own
    // environment, so the env fallbacks are cleared and only argv is parsed.
    let cmd = hunch::cli::Cli::command().mut_args(|a| a.env(None));
    let m = cmd
        .try_get_matches_from(["hunch", "run", "burn", "a", "dvd", "--dry-run", "--json"])
        .unwrap();
    let c = hunch::cli::Cli::from_arg_matches(&m).unwrap();
    assert!(c.g.json);
    let hunch::cli::Cmd::Run {
        intent, dry_run, ..
    } = c.cmd
    else {
        unreachable!("expected run")
    };
    assert!(dry_run);
    assert_eq!(intent, ["burn", "a", "dvd"]);
}
