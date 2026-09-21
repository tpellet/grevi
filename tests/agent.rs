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
    for verb in ["pick", "why", "route", "filter", "is"] {
        assert!(
            d["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == verb),
            "{verb}"
        );
    }
    // Wave 2: both `add` (Task 14) and `sort` (Task 15) are listed.
    for verb in ["add", "sort"] {
        assert!(
            d["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == verb),
            "{verb}"
        );
    }
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
