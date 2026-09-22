mod common;

#[tokio::test(flavor = "multi_thread")]
async fn documented_input_side_shell_examples_run_against_the_binary() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, options| {
            options
                .iter()
                .find(|s| s.as_str() != "NONE")
                .unwrap()
                .clone()
        },
        noul: |_, _| 0.9,
    })
    .await;
    let binary_dir = std::path::Path::new(env!("CARGO_BIN_EXE_jevify"))
        .parent()
        .unwrap();
    let mut paths = vec![binary_dir.to_path_buf()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let path = std::env::join_paths(paths).unwrap();
    let mut count = 0;
    for document in [
        include_str!("../README.md"),
        include_str!("../docs/ROBOT_MODE.md"),
        include_str!("../docs/guide/getting-started.md"),
        include_str!("../docs/guide/verbs.md"),
        include_str!("../plugins/jevify/skills/jevify/SKILL.md"),
    ] {
        let mut in_shell = false;
        let mut command = String::new();
        for line in document.lines() {
            if line.starts_with("```") {
                in_shell = line == "```sh";
                continue;
            }
            if !in_shell {
                continue;
            }
            command.push_str(line);
            command.push('\n');
            if line.ends_with('\\') {
                continue;
            }
            if command.contains("jevify fill") || command.contains("jevify pick --from") {
                // Documentation previews must never execute the command being illustrated.
                assert!(!command.contains("jevify fill") || command.contains("--dry-run"));
                let out = std::process::Command::new("sh")
                    .args(["-c", &command])
                    .env_clear()
                    .env("PATH", &path)
                    .env("JEVIFY_BACKEND", "typesafe")
                    .env("TYPESAFE_API_KEY", "test-key")
                    .env("JEVIFY_BASE_URL", server.uri())
                    .env("JEVIFY_NO_CACHE", "1")
                    .stdin(std::process::Stdio::null())
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "{command}\n{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert!(!out.stdout.is_empty(), "{command}");
                count += 1;
            }
            command.clear();
        }
    }
    assert!(count >= 15, "only {count} documented examples exercised");
}

#[tokio::test]
async fn health_never_follows_redirects() {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let target = MockServer::start().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", target.uri()))
        .expect(1)
        .mount(&server)
        .await;
    let endpoint = server.uri();
    let out = tokio::task::spawn_blocking(move || {
        common::bin()
            .env("TYPESAFE_API_KEY", "test-key")
            .env("JEVIFY_BASE_URL", endpoint)
            .args(["health", "--json"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(4));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["exit_code"], 4);
    assert!(target.received_requests().await.unwrap().is_empty());
}

#[test]
fn capabilities_lists_verbs_exit_codes_env() {
    let out = common::bin()
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = &v["data"];
    use clap::CommandFactory;
    let parser = jevify::cli::Cli::command();
    let verbs: Vec<_> = parser.get_subcommands().map(|c| c.get_name()).collect();
    let commands = d["commands"].as_array().unwrap();
    assert_eq!(
        commands
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        verbs
    );
    for command in commands {
        assert!(!command["usage"].as_str().unwrap().is_empty());
        assert!(!command["data"].as_str().unwrap().is_empty());
        let codes = command["exit"].as_array().unwrap();
        assert!(!codes.is_empty());
        assert!(codes.iter().all(|code| code.is_u64()));
    }
    assert_eq!(d["limits"]["distinct_records"], 20_000);
    assert_eq!(d["limits"]["records_per_request"]["classifier"], 1_000);
    assert_eq!(d["limits"]["records_per_request"]["typesafe"], 20);
    assert_eq!(
        d["saved_inputs"]["verbs"],
        serde_json::json!(["why", "filter"])
    );
    assert!(
        d["saved_inputs"]["directory"]
            .as_str()
            .unwrap()
            .contains("outputs")
    );
    assert_eq!(d["exit_codes"].as_array().unwrap().len(), 9);
    assert!(
        d["env"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "TYPESAFE_API_KEY")
    );
    assert_eq!(d["limits"]["choice_options"], 255);
    let fill = commands.iter().find(|c| c["name"] == "fill").unwrap();
    assert!(
        fill["data"]
            .as_str()
            .unwrap()
            .contains("markers[{arg,kind,reason")
    );
    assert!(
        fill["exit_meaning"]
            .as_str()
            .unwrap()
            .contains("command's own exit code")
    );
    let pick = commands.iter().find(|c| c["name"] == "pick").unwrap();
    assert!(pick["usage"].as_str().unwrap().contains("pick --from KIND"));
    assert_eq!(
        d["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["-", "branch", "one", "flag"]
    );
    assert_eq!(
        d["kinds"][1]["list"],
        serde_json::json!([
            "git",
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname)%00%(symref)%00%(committerdate:unix)%00%(subject)%00",
            "refs/heads",
            "refs/remotes"
        ])
    );
    assert_eq!(
        d["kinds"][1]["enrich"],
        serde_json::json!([
            "git",
            "log",
            "-5",
            "--format=%x00%s%x00",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--end-of-options",
            "<handle>",
            "--"
        ])
    );
    for (backend, window, fill, pick) in [
        ("typesafe", 200, 13200, 20000),
        ("classifier", 99, 3267, 9801),
    ] {
        let limits = &d["selection_limits"][backend];
        assert_eq!(limits["window"], window);
        assert_eq!(limits["fill"], fill);
        assert_eq!(limits["pick"], pick);
        assert_eq!(limits["one_options"], window);
    }
    assert!(
        d["selection_limits"]["finalists_per_window"]
            .as_str()
            .unwrap()
            .contains("else 2")
    );
    assert!(
        d["selection_limits"]["finalists_per_window"]
            .as_str()
            .unwrap()
            .contains("else 1")
    );
    assert_eq!(
        d["input_errors"]["kinds"],
        serde_json::json!([
            "stdin_is_tty",
            "lister_failed",
            "too_many",
            "cannot_run",
            "recipe_invalid"
        ])
    );
    assert_eq!(d["input_errors"]["field"], "error.kind");
    assert_eq!(d["input_errors"]["exit"], 6);
    assert_eq!(
        d["fill_abstention"]["reasons"],
        serde_json::json!([
            "no_match",
            "ambiguous",
            "unsure_flag",
            "insufficient_evidence"
        ])
    );
    assert_eq!(d["fill_abstention"]["exit"], 3);
    assert!(d["fill_abstention"]["error"].is_null());
    assert_eq!(d["fill_abstention"]["field"], "data.reason");
    assert_eq!(
        d["fill_abstention"]["marker_field"],
        "data.markers[].reason"
    );
    assert_eq!(d["fill_model_guard"]["missing_model"], "unknown");
    assert_eq!(d["fill_model_guard"]["kind"], "api_unavailable");
    assert_eq!(d["fill_model_guard"]["exit"], 4);
    assert_eq!(
        d["fill_model_guard"]["message"],
        "answered by <model>, not Jev"
    );
    assert_eq!(
        d["env"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == "JEVIFY_MODEL")
            .unwrap()["default"],
        "jev-1.13.0"
    );
}

#[test]
fn agent_block_is_bounded_and_public_help_has_no_removed_forms() {
    use clap::CommandFactory;
    let parser = jevify::cli::Cli::command();
    let verbs: Vec<_> = parser.get_subcommands().map(|c| c.get_name()).collect();
    let out = common::bin().args(["init", "agents"]).output().unwrap();
    assert!(out.status.success());
    let block = String::from_utf8(out.stdout).unwrap();
    assert!(block.lines().count() <= 25);
    assert!(block.contains("jevify capabilities --json"));
    assert!(block.contains("yes means act"));
    assert!(block.contains("'@{branch:the auth refactor}'"));
    assert!(block.contains("Quote the whole marker argument"));
    for verb in &verbs {
        assert!(block.contains(&format!("- {verb}:")), "{verb}");
    }
    let mut outputs = vec![block];
    for args in [vec!["--help"], vec!["capabilities", "--json"]]
        .into_iter()
        .chain(verbs.iter().map(|verb| vec![*verb, "--help"]))
    {
        let out = common::bin().args(&args).output().unwrap();
        assert!(out.status.success(), "{args:?}");
        outputs.push(String::from_utf8(out.stdout).unwrap());
    }
    let removed = regex::Regex::new(
        r"jevify (?:run|label)(?:\s|$)|why -- |jevify -v(?:\s|$)|@\{(?:commit|file|dir|tool|pod|pr|issue|ci-run|stash|process|container):",
    ).unwrap();
    for allowed in [
        "jevify fill",
        "jevify pick --from branch",
        "'@{-:x}'",
        "'@{branch:x}'",
        "'@{one:a|b:x}'",
        "'@{flag:--draft:x}'",
    ] {
        assert!(!removed.is_match(allowed), "{allowed}");
    }
    for forbidden in [
        "jevify label x",
        "'@{commit:x}'",
        "'@{file:x}'",
        "'@{dir:x}'",
        "'@{tool:x}'",
        "'@{pod:x}'",
        "'@{pr:x}'",
    ] {
        assert!(removed.is_match(forbidden), "{forbidden}");
    }
    for output in outputs {
        assert!(!removed.is_match(&output), "{output}");
    }
    for shell in ["bash", "zsh"] {
        let out = common::bin().args(["init", shell]).output().unwrap();
        assert!(out.status.success());
        let script = String::from_utf8(out.stdout).unwrap();
        assert!(script.contains("alias ,=") && script.contains("jevify route"));
        assert!(script.contains("then jevify route \"$*\""));
    }
}

#[test]
fn robot_docs_topics() {
    for t in ["guide", "commands", "exit-codes", "examples", "privacy"] {
        common::bin().args(["robot-docs", t]).assert().success();
    }
    common::bin().args(["robot-docs", "nope"]).assert().code(2);
}

/// Only the backend that needs a key can fail for want of one; with no key jevify uses
/// classifier.dev instead, which `tests/classifier.rs` covers against a mock server.
#[test]
fn health_without_key_is_auth_error_on_the_typesafe_backend() {
    common::bin()
        .env("JEVIFY_BACKEND", "typesafe")
        .args(["health", "--json"])
        .assert()
        .code(5);
}

// A bad key file must not break verbs that need no key.
#[test]
fn capabilities_work_with_an_unreadable_key_file() {
    common::bin()
        .env("TYPESAFE_API_KEY_FILE", "/nonexistent/jevify-key")
        .args(["capabilities", "--json"])
        .assert()
        .success();
}
