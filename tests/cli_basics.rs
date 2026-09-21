use assert_cmd::Command;
use assert_cmd::cargo::CommandCargoExt;
mod common;

#[tokio::test(flavor = "multi_thread")]
async fn health_json_reports_get_attempts_and_no_inference() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.8,
    })
    .await;
    let out = tokio::task::spawn_blocking(move || {
        common::jevify(&server)
            .args(["health", "--json"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["meta"]["requests"], 0);
    assert_eq!(value["meta"]["telemetry"]["health_gets"]["succeeded"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_usage_is_null_in_json_and_unknown_in_verbose_output() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"answers":{"is":{"noul":0.8}}})),
        )
        .mount(&server)
        .await;
    tokio::task::spawn_blocking(move || {
        let out = common::jevify(&server)
            .args(["is", "condition", "--json"])
            .write_stdin("x")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["meta"]["input_tokens"], serde_json::Value::Null);
        assert_eq!(value["meta"]["cost_usd"], serde_json::Value::Null);
        assert_eq!(
            value["meta"]["telemetry"]["usage"]["input_tokens"]["unknown_attempts"],
            1
        );
        let out = common::jevify(&server)
            .args(["is", "condition", "--verbose"])
            .write_stdin("x")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&out.stderr).contains("cost unknown"));
    })
    .await
    .unwrap();
}

#[test]
fn closed_stdout_is_normal_for_output_and_json_errors() {
    for args in [vec!["capabilities"], vec!["--json", "pick", "--nope"]] {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let out = std::process::Command::cargo_bin("jevify")
            .unwrap()
            .args(args)
            .stdout(writer)
            .stderr(std::process::Stdio::piped())
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
    }
}

#[test]
fn help_lists_every_verb() {
    let out = Command::cargo_bin("jevify")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    for verb in [
        "pick",
        "why",
        "route",
        "filter",
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
        text.contains("Stage only") && text.contains("Propose a folder"),
        "{text}"
    );
}

// Bare `jevify` is a usage error with a short card: every verb, the exit codes, the agent entry
// points, and small enough to cost an agent about 130 tokens.
#[test]
fn bare_jevify_prints_the_quick_start_card_as_a_usage_error() {
    let out = Command::cargo_bin("jevify").unwrap().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let text = String::from_utf8(out.stderr).unwrap();
    for needle in [
        "jevify pick",
        "jevify why",
        "jevify is",
        "jevify route",
        "jevify filter",
        "jevify add",
        "jevify sort",
        "--json",
        "3 nothing fits or unsure",
        "jevify capabilities --json",
    ] {
        assert!(text.contains(needle), "{needle} missing from:\n{text}");
    }
    assert!(text.len() < 1000, "{} bytes", text.len());
    assert!(!text.contains("jevify run"));
}

// An agent that runs `jevify <verb> --help` gets an example to copy and the exit codes, and the
// free-text argument says how to phrase it.
#[test]
fn verb_help_has_examples_exit_codes_and_a_described_argument() {
    for (verb, arg) in [
        ("pick", "Describe the line"),
        ("is", "A statement that must be true"),
        ("add", "The topic of the changes"),
    ] {
        let out = Command::cargo_bin("jevify")
            .unwrap()
            .args([verb, "--help"])
            .output()
            .unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        for needle in ["Examples:", "Exit:", arg] {
            assert!(
                text.contains(needle),
                "{verb} --help lacks {needle}:\n{text}"
            );
        }
    }
}

#[test]
fn unknown_flag_is_usage_error() {
    Command::cargo_bin("jevify")
        .unwrap()
        .args(["pick", "--nope", "x"])
        .assert()
        .code(2);
}

// An out-of-range threshold is rejected by Config::load for every verb, so this stays a
// usage error for the life of the project (unlike a not-yet-implemented verb).
#[test]
fn json_error_envelope_has_kind_hint_example() {
    let out = Command::cargo_bin("jevify")
        .unwrap()
        .args(["--json", "-t", "2", "is", "x"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["exit_code"], 2);
    assert_eq!(v["error"]["kind"], "usage");
    assert!(v["error"]["example"].as_str().unwrap().contains("jevify"));
}

// Clap fails before any Config exists; agents still get exactly one envelope on stdout.
#[test]
fn clap_usage_error_under_json_is_an_envelope() {
    let out = Command::cargo_bin("jevify")
        .unwrap()
        .args(["--json", "pick", "--nope", "x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["kind"], "usage");
    assert_eq!(v["command"], "pick");
    let out = Command::cargo_bin("jevify")
        .unwrap()
        .args(["pick", "--format", "toon", "--nope", "x"])
        .output()
        .unwrap();
    assert!(String::from_utf8(out.stdout).unwrap().contains("ok: false"));
}

#[test]
fn toon_format_renders() {
    let out = Command::cargo_bin("jevify")
        .unwrap()
        .args(["--format", "toon", "-t", "2", "is", "x"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("ok: false"), "{s}");
}

// `trailing_var_arg` would swallow these into the intent (clap_builder Arg::trailing_var_arg docs).
#[test]
fn route_text_after_intent_is_text_and_removed_flags_are_errors() {
    use clap::{CommandFactory, FromArgMatches};
    // In-process parse: clap would read `env = "JEVIFY_THRESHOLD"` from this test's own
    // environment, so the env fallbacks are cleared and only argv is parsed.
    let cmd = jevify::cli::Cli::command().mut_args(|a| a.env(None));
    let m = cmd
        .try_get_matches_from(["jevify", "route", "burn", "a", "dvd", "--json"])
        .unwrap();
    let c = jevify::cli::Cli::from_arg_matches(&m).unwrap();
    assert!(c.g.json);
    let jevify::cli::Cmd::Route { intent } = c.cmd else {
        unreachable!("expected route")
    };
    assert_eq!(intent, ["burn", "a", "dvd"]);
    assert!(
        jevify::cli::Cli::command()
            .mut_args(|a| a.env(None))
            .try_get_matches_from(["jevify", "route", "burn", "--yes"])
            .is_err()
    );
}

#[test]
fn removed_commands_report_one_corrected_line_and_usage_envelopes() {
    for (args, command, correction) in [
        (vec!["run", "x"], "jevify", "jevify route 'x'"),
        (vec!["why", "--", "true"], "why", "CMD 2>&1 | jevify why"),
    ] {
        let out = common::bin().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(stderr.lines().count(), 1);
        assert!(stderr.contains(correction));
        let out = common::bin().arg("--json").args(&args).output().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(value["exit_code"], 2);
        assert_eq!(value["command"], command);
        assert_eq!(value["error"]["kind"], "usage");
        assert_eq!(value["meta"]["requests"], 0);
    }
}

#[test]
fn clap_errors_preserve_non_utf8_args_and_stop_format_scanning_at_double_dash() {
    use std::os::unix::ffi::OsStringExt;
    for json in [false, true] {
        let mut cmd = common::bin();
        if json {
            cmd.arg("--json");
        }
        let out = cmd
            .args(["pick", "q", "--", "--json"])
            .arg(std::ffi::OsString::from_vec(vec![0xff]))
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
        if json {
            let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(value["error"]["kind"], "usage");
        } else {
            assert!(out.stdout.is_empty());
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn interim_refusals_make_no_requests() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.9,
    })
    .await;
    for args in [
        vec!["pick", "-0", "q"],
        vec!["pick", "--para", "q"],
        vec!["is", "a", "b"],
        vec!["is", "a", "--context", "file"],
        vec!["filter", "x"],
        vec!["filter", "-v", "x"],
    ] {
        let mut cmd = common::jevify(&server);
        let out = tokio::task::spawn_blocking(move || {
            cmd.arg("--json")
                .args(args)
                .write_stdin("input")
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(out.status.code(), Some(6));
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("not implemented")
        );
        assert_eq!(value["meta"]["requests"], 0);
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[test]
fn all_help_uses_the_cutover_grammar() {
    for verb in [
        "",
        "pick",
        "why",
        "route",
        "filter",
        "is",
        "add",
        "sort",
        "capabilities",
        "robot-docs",
        "health",
        "init",
    ] {
        let mut cmd = common::bin();
        if !verb.is_empty() {
            cmd.arg(verb);
        }
        let out = cmd.arg("--help").output().unwrap();
        assert!(out.status.success());
        let text = String::from_utf8(out.stdout).unwrap();
        for removed in [
            "jevify run",
            "why -- ",
            "-- <cmd>",
            "--exec",
            "--no-args",
            "--files DIR",
            "--files <DIR>",
            "child_exit",
            "jevify -v",
        ] {
            assert!(!text.contains(removed), "{verb}: {removed}");
        }
    }
}
