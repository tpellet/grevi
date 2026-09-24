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
    // The examples name branches, commits and files under `src/`. A tag checkout in CI has
    // no branch refs, so the examples run in a repository of their own with known content.
    let repo = example_repository();
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
            // `label` examples that start from `printf` need no tool of the machine, so they
            // run too: the fake answers the first label, and every record comes out.
            if command.contains("jevify fill")
                || command.contains("jevify pick --from")
                || (command.starts_with("printf") && command.contains("jevify label"))
            {
                // Documentation previews must never execute the command being illustrated.
                assert!(!command.contains("jevify fill") || command.contains("--dry-run"));
                let out = std::process::Command::new("sh")
                    .args(["-c", &command])
                    .current_dir(&repo)
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

/// A git repository with two commits, two branches and files under `src/`: everything the
/// documented `fill` and `pick --from` examples list, independent of the checkout running the test.
fn example_repository() -> std::path::PathBuf {
    let root = tempfile::tempdir().unwrap().keep();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .current_dir(&root)
            .args([
                "-c",
                "user.name=example",
                "-c",
                "user.email=example@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("src/marker.rs"), "pub fn parse_marker() {}\n").unwrap();
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "parse the marker"]);
    std::fs::write(root.join("src/moves.rs"), "pub fn move_all() {}\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "make folder moves atomic"]);
    git(&["branch", "auth-refactor"]);
    git(&["branch", "payment-timeout"]);
    root
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
    let label = commands.iter().find(|c| c["name"] == "label").unwrap();
    assert_eq!(label["exit"], serde_json::json!([0, 2, 3, 4, 6]));
    assert_eq!(
        label["usage"],
        "CMD | jevify label a,b,c [-0|--para] [--files]"
    );
    assert!(label["data"].as_str().unwrap().starts_with(
        "records[{label,text,ordinal,p,lossy?,unreadable?}], labelled, total, unsure"
    ));
    let note = label["note"].as_str().unwrap();
    assert!(note.contains("? marks an unsure record"), "{note}");
    assert!(
        note.contains("99 on classifier.dev, 200 on TypeSafe"),
        "{note}"
    );
    assert!(
        label["example"]
            .as_str()
            .unwrap()
            .contains("jevify label bug,feature,question")
    );
    assert_eq!(d["limits"]["distinct_records"], 20_000);
    // The keyless quota, per verb, from the measurement in benchmarks/results.md.
    let classifier = d["backends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == "classifier")
        .unwrap();
    assert_eq!(classifier["classifications_per_minute"], 3_000);
    assert_eq!(classifier["classifications_per_day"], 20_000);
    assert!(
        classifier["classification"]
            .as_str()
            .unwrap()
            .contains("per IP")
    );
    for verb in ["is", "filter", "label", "pick", "why", "route"] {
        assert!(classifier["cost_per_call"][verb].is_string(), "{verb}");
        assert!(classifier["calls_per_day"][verb].is_string(), "{verb}");
    }
    assert!(
        classifier["basis"]
            .as_str()
            .unwrap()
            .contains("benchmarks/results.md")
    );
    assert_eq!(d["limits"]["records_per_request"]["classifier"], 60);
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
    // The store is bounded and an operator can turn it off without touching the call sites.
    assert!(
        d["saved_inputs"]["retention"]
            .as_str()
            .unwrap()
            .contains("7 days")
    );
    assert!(
        d["saved_inputs"]["disable"]
            .as_str()
            .unwrap()
            .contains("JEVIFY_NO_SAVE=1")
    );
    assert!(
        d["env"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "JEVIFY_NO_SAVE")
    );
    // `complete` says where it stops, next to the fields that do mean coverage.
    for (verb, coverage) in [("why", "considered against total"), ("filter", "unsure")] {
        let data = d["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == verb)
            .unwrap()["data"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(data.contains("complete ("), "{verb}: {data}");
        assert!(data.contains(coverage), "{verb}: {data}");
    }
    assert_eq!(d["exit_codes"].as_array().unwrap().len(), 9);
    assert!(
        d["env"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "TYPESAFE_API_KEY")
    );
    let deadline = d["env"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "JEVIFY_DEADLINE")
        .unwrap();
    assert_eq!(deadline["default"], 600);
    assert!(d["envelope"]["fields"].as_array().unwrap().iter().any(|f| {
        f.as_str()
            .unwrap()
            .contains("request_id,usage,telemetry,decision}")
    }));
    // The decision structure is stated the same way in capabilities and in ROBOT_MODE.md.
    let decision = d["envelope"]["decision"]["fields"].as_str().unwrap();
    assert_eq!(
        decision,
        "decision{verb,backend,model{requested,answering},threshold,gates[{best,next,none,any,fails}],round_one?[{windows[{ranks[{index,p}],none,any}],finalists[],n}]}"
    );
    let robot_docs = include_str!("../docs/ROBOT_MODE.md");
    assert!(robot_docs.contains(
        "decision{verb, backend, model{requested, answering}, threshold, gates[{best, next, none, any, fails}],\n         round_one?[{windows[{ranks[{index, p}], none, any}], finalists[], n}]}"
    ));
    for field in [
        "verb",
        "backend",
        "model{requested,answering}",
        "threshold",
        "gates",
        "round_one",
    ] {
        assert!(decision.contains(field), "{field}");
    }
    // round_one is opt-in, and its finalists are the finals as sent: capabilities, ROBOT_MODE
    // and the env list say so in the same words.
    assert!(robot_docs.contains("`round_one` is present only under `JEVIFY_DECISION=round_one`"));
    assert!(robot_docs.contains("`finalists`, the items the finals request held, in its order"));
    // Prose the document wraps: compare with the line breaks folded into single spaces, so a
    // reflow does not fail the test and a changed sentence still does.
    let flowed = robot_docs.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flowed.contains("whether the finals judged it, reads from one run"));
    let round_one = d["envelope"]["decision"]["round_one"].as_str().unwrap();
    assert!(
        round_one.starts_with("present only under JEVIFY_DECISION=round_one"),
        "{round_one}"
    );
    assert!(
        round_one.contains("finalists, the items the finals request held, in its order"),
        "{round_one}"
    );
    for step in [
        "widened by fill",
        "why's panic lines",
        "capped at 12 by route",
    ] {
        assert!(round_one.contains(step), "{round_one}");
    }
    let decision_env = d["env"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "JEVIFY_DECISION")
        .unwrap();
    assert!(
        decision_env["meaning"]
            .as_str()
            .unwrap()
            .starts_with("round_one: add meta.decision.round_one")
    );
    assert!(robot_docs.contains("`model.answering`") && robot_docs.contains("`unknown`"));
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
    // bin() points JEVIFY_CONFIG_DIR at an empty directory, so no user recipe is listed.
    assert_eq!(
        d["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "-",
            "branch",
            "commit",
            "file",
            "dir",
            "tool",
            "pr",
            "issue",
            "ci-run",
            "stash",
            "process",
            "container",
            "pod",
            "one",
            "flag"
        ]
    );
    assert!(d["kinds_error"].is_null());
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
    let forms = d["kinds"][1]["forms"].as_str().unwrap();
    assert!(
        forms.contains("bare '@{branch:x}' substitutes the short name"),
        "{forms}"
    );
    assert!(forms.contains("git switch"), "{forms}");
    assert!(
        forms.contains(
            "'origin/@{branch:x}' lists that remote's refs and substitutes the qualified ref"
        ),
        "{forms}"
    );
    assert!(forms.contains("git log, rev-parse"), "{forms}");
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
    // `fill` keeps three names in the shortlist round; an undecided single window sends every
    // name with p > 0 to the finals, up to 24, for the four kinds with tier-two evidence.
    let finalists = d["selection_limits"]["finalists_per_window"]
        .as_str()
        .unwrap();
    assert!(finalists.contains("up to 24"), "{finalists}");
    assert!(
        finalists.contains("branch, commit, file or dir"),
        "{finalists}"
    );
    // Exit 7 is reserved: no verb reports a child command's failure, and the name must not
    // read as if one did.
    let seven = d["exit_codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["code"] == 7)
        .unwrap();
    assert_eq!(seven["name"], "reserved");
    assert!(
        seven["meaning"]
            .as_str()
            .unwrap()
            .starts_with("reserved, never returned"),
        "{seven}"
    );
    // Every exit-6 kind, not the five that `fill` alone raises: a caller enumerates its
    // branches from the machine interface.
    assert_eq!(
        d["input_errors"]["kinds"],
        serde_json::json!([
            "empty_input",
            "input_too_large",
            "api_rejected_request",
            "input",
            "too_many",
            "stdin_is_tty",
            "lister_failed",
            "cannot_run",
            "recipe_invalid",
            "status_file_unwritable"
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
    assert!(block.contains("label prints LABEL<TAB>RECORD"));
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
        r"jevify run(?:\s|$)|why -- |jevify -v(?:\s|$)|@\{(?:test|script|complete):",
    )
    .unwrap();
    for allowed in [
        "jevify fill",
        "jevify pick --from branch",
        "jevify pick --from commit",
        "jevify label x",
        "'@{-:x}'",
        "'@{branch:x}'",
        "'@{commit:x}'",
        "'@{file:x}'",
        "'@{dir:x}'",
        "'@{tool:x}'",
        "'@{pod:x}'",
        "'@{pr:x}'",
        "'@{one:a|b:x}'",
        "'@{flag:--draft:x}'",
    ] {
        assert!(!removed.is_match(allowed), "{allowed}");
    }
    for forbidden in [
        "jevify run x",
        "why -- cargo build",
        "'@{test:x}'",
        "'@{script:x}'",
        "'@{complete:x}'",
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

/// Every coded kind and every shipped recipe is listed with the argv of its lister, and a user
/// recipe from `JEVIFY_CONFIG_DIR` joins them with origin `user`; a bad user file is reported in
/// `kinds_error` and never fails `capabilities`.
#[test]
fn capabilities_list_every_kind_with_its_argv_and_the_user_recipes() {
    let shipped: Vec<serde_json::Value> = include_str!("../src/kinds.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(shipped.len(), 7);
    let dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(
        dir.join("kinds.jsonl"),
        "{\"kind\":\"widget\",\"list\":[\"printf\",\"w1\\\\n\"],\"ordered\":true}\n",
    )
    .unwrap();
    let out = common::bin()
        .env("JEVIFY_CONFIG_DIR", &dir)
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let kinds = v["data"]["kinds"].as_array().unwrap();
    let entry = |name: &str| kinds.iter().find(|k| k["name"] == name).unwrap();
    for (name, argv) in [
        ("branch", vec!["git", "for-each-ref"]),
        (
            "commit",
            vec!["git", "log", "-z", "--format=%H%x00%s", "HEAD", "--"],
        ),
        (
            "file",
            vec!["git", "ls-files", "-co", "--exclude-standard", "-z"],
        ),
        (
            "dir",
            vec!["git", "ls-files", "-co", "--exclude-standard", "-z"],
        ),
    ] {
        let kind = entry(name);
        assert_eq!(kind["origin"], "coded", "{kind}");
        let list: Vec<&str> = kind["list"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert!(list.starts_with(&argv), "{kind}");
    }
    for recipe in &shipped {
        let kind = entry(recipe["kind"].as_str().unwrap());
        assert_eq!(kind["origin"], "shipped", "{kind}");
        assert_eq!(kind["list"], recipe["list"], "{kind}");
        assert_eq!(
            kind["ordered"],
            recipe["ordered"].as_bool().unwrap_or(false),
            "{kind}"
        );
    }
    let widget = entry("widget");
    assert_eq!(widget["origin"], "user");
    assert_eq!(widget["list"], serde_json::json!(["printf", "w1\\n"]));
    assert_eq!(widget["ordered"], true);
    assert!(v["data"]["kinds_error"].is_null());
    let position = |name: &str| kinds.iter().position(|k| k["name"] == name).unwrap();
    assert!(position("pod") < position("widget"));
    assert!(position("widget") < position("one"));
    assert_eq!(
        v["data"]["withheld"]["patterns"],
        serde_json::json!([".*", "id_*", "*.pem", "*.key", "*credentials*", "*secret*"])
    );
    assert!(
        v["data"]["env"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "JEVIFY_CONFIG_DIR")
    );

    // A bad file: the shipped kinds are still listed, and the reason names the line.
    std::fs::write(dir.join("kinds.jsonl"), "{\"kind\":\"widget\"}\n").unwrap();
    let out = common::bin()
        .env("JEVIFY_CONFIG_DIR", &dir)
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let kinds = v["data"]["kinds"].as_array().unwrap();
    assert!(kinds.iter().any(|k| k["name"] == "pod"));
    assert!(kinds.iter().all(|k| k["name"] != "widget"));
    assert!(
        v["data"]["kinds_error"]
            .as_str()
            .unwrap()
            .contains("line 1"),
        "{}",
        v["data"]["kinds_error"]
    );
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

/// `error.kind` is enumerated in the machine interface, and the enumeration is the one table in
/// `src/exit.rs`. The three checks below close the three ways the two could drift: a kind the
/// table forgets (the library test `the_published_kind_table_is_exactly_what_the_errors_produce`
/// catches it), a kind written as a literal at a `Kinded` site and never added to the table
/// (the source scan here catches it), and a table `capabilities` does not publish (the equality
/// here catches it).
#[test]
fn capabilities_enumerates_every_error_kind_and_cannot_drift_from_exit_rs() {
    let out = common::bin()
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let published: Vec<(String, i64)> = v["data"]["error_kinds"]
        .as_array()
        .expect("capabilities enumerates error.kind")
        .iter()
        .map(|e| {
            (
                e["kind"].as_str().unwrap().to_string(),
                e["exit"].as_i64().unwrap(),
            )
        })
        .collect();
    let table: Vec<(String, i64)> = jevify::exit::JevifyError::KINDS
        .iter()
        .map(|(kind, exit)| (kind.to_string(), i64::from(exit.code())))
        .collect();
    assert_eq!(published, table, "capabilities publishes exit.rs's table");
    assert!(
        published
            .iter()
            .any(|(k, e)| k == "api_deadline" && *e == 4)
    );
    assert!(
        published
            .iter()
            .any(|(k, e)| k == "status_file_unwritable" && *e == 6)
    );

    // Every `kind: "..."` literal anywhere in the sources is one a caller can receive.
    let names: std::collections::BTreeSet<&str> =
        table.iter().map(|(kind, _)| kind.as_str()).collect();
    let mut scanned = 0;
    let mut stack = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for (number, line) in source.lines().enumerate() {
                let Some(rest) = line.trim_start().strip_prefix("kind: \"") else {
                    continue;
                };
                let literal = rest.split('"').next().unwrap();
                scanned += 1;
                assert!(
                    names.contains(literal),
                    "{}:{}: kind {literal:?} is not in JevifyError::KINDS",
                    path.display(),
                    number + 1
                );
            }
        }
    }
    assert!(
        scanned >= 6,
        "the source scan found {scanned} kind literals"
    );

    // Every exit code lists the kinds that carry it, and the three readings of exit 4 are apart.
    let four = v["data"]["exit_codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["code"] == 4)
        .unwrap();
    assert_eq!(
        four["kinds"],
        serde_json::json!(["api_unavailable", "api_deadline", "api_protocol"])
    );
}

/// The per-request read timeout is part of what a caller waits for, so it is published.
#[test]
fn capabilities_state_the_per_request_timeouts_the_client_uses() {
    let out = common::bin()
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["limits"]["request_read_timeout_s"], 60);
    assert_eq!(v["data"]["limits"]["connect_timeout_s"], 5);
    // The published numbers are the client's own.
    let client = include_str!("../src/jev/client.rs");
    assert!(
        client.contains(".timeout(Duration::from_secs(60))"),
        "the read timeout moved: update capabilities.limits and docs/ROBOT_MODE.md"
    );
    assert!(
        client.contains(".connect_timeout(Duration::from_secs(5))"),
        "the connect timeout moved: update capabilities.limits and docs/ROBOT_MODE.md"
    );
}

/// The exec-mode signal is described where a machine reads it.
#[test]
fn capabilities_describe_the_exec_status_file() {
    let out = common::bin()
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let status = &v["data"]["exec_status"];
    assert_eq!(status["env"], "JEVIFY_STATUS_FILE");
    assert_eq!(status["verb"], "fill");
    assert_eq!(
        status["fields"],
        serde_json::json!([
            "command",
            "version",
            "exit_code",
            "ran",
            "argv",
            "reason",
            "markers",
            "error"
        ])
    );
    assert!(status["rule"].as_str().unwrap().contains("ran true"));
    assert!(
        v["data"]["env"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "JEVIFY_STATUS_FILE")
    );
}
