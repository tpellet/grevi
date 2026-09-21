mod common;

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
    let removed =
        regex::Regex::new(r"jevify (?:run|label)(?:\s|$)|why -- |jevify -v(?:\s|$)").unwrap();
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
