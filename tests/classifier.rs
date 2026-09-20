//! The classifier.dev backend: same verbs, same envelope, same exit codes, no key.
mod common;

#[test]
fn explicit_classifier_model_overrides_are_usage_errors() {
    for via_env in [false, true] {
        let mut command = common::bin();
        command
            .env("JEVIFY_BACKEND", "classifier")
            .args(["--json", "capabilities"]);
        if via_env {
            command.env("JEVIFY_MODEL", "jev-1.13.0");
        } else {
            command.args(["--model", "jev-1.13.0"]);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["kind"], "usage");
    }
}
use common::{FakeJev, option_containing};
use jevify::config::Backend;
use jevify::jev::client::Client;
use jevify::jev::{Question, Questions};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn classifier_config(server: &MockServer) -> jevify::config::Config {
    let mut c = common::config(server);
    c.backend = Backend::Classifier;
    c.key = None;
    c
}

fn one_noul() -> Questions {
    let mut q = Questions::new();
    q.insert("q".into(), Question::noul("Is it?"));
    q
}

#[tokio::test(flavor = "multi_thread")]
async fn pick_works_with_no_key_at_all() {
    let server = common::mock_classifier(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::jevify_classifier(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "pick", "the bill"])
            .write_stdin("notes.txt\ninvoice-march.pdf\nphoto.jpg\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["matches"][0]["text"], "invoice-march.pdf");
    // The envelope says which API answered, and with which model.
    assert_eq!(v["meta"]["backend"], "classifier");
    assert_eq!(v["meta"]["model"], "jev-fake");
    // Free pricing is explicit; missing usage is not measured zero.
    assert_eq!(v["meta"]["input_tokens"], serde_json::Value::Null);
    assert_eq!(v["meta"]["cost_usd"], 0.0);
    assert_eq!(
        v["meta"]["telemetry"]["cost_estimate"]["basis"],
        "free_service"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn is_keeps_its_exit_codes_on_the_free_backend() {
    let server = common::mock_classifier(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, s| {
            if s.as_str().unwrap_or_default().contains("error") {
                0.95
            } else {
                0.02
            }
        },
    })
    .await;
    for (stdin, code) in [("a fatal error occurred", 0), ("all fine", 1)] {
        let mut c = common::jevify_classifier(&server);
        let out = tokio::task::spawn_blocking(move || {
            c.args(["is", "this text reports a failure"])
                .write_stdin(stdin)
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(out.status.code(), Some(code), "{stdin}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn health_reports_the_active_backend_without_a_key() {
    let server = common::mock_classifier(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.5,
    })
    .await;
    let mut c = common::jevify_classifier(&server);
    let out = tokio::task::spawn_blocking(move || c.args(["--json", "health"]).output().unwrap())
        .await
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["backend"], "classifier");
    assert_eq!(v["data"]["key"], "not needed");
    assert_eq!(v["data"]["api"], "reachable");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_key_and_no_backend_variable_still_answers() {
    // What a new user gets: `cargo install jevify` and nothing else. Only the base URL is
    // overridden, so the backend choice itself is the one jevify makes from an empty environment.
    let server = common::mock_classifier(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::bin();
    c.env("JEVIFY_BASE_URL", server.uri())
        .env("JEVIFY_NO_CACHE", "1");
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "pick", "the bill"])
            .write_stdin("notes.txt\ninvoice-march.pdf\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["meta"]["backend"], "classifier");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_typesafe_backend_without_a_key_is_an_auth_error() {
    let mut c = common::bin();
    c.env("JEVIFY_BACKEND", "typesafe");
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "pick", "x"])
            .write_stdin("a\nb\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(5));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["kind"], "missing_api_key");
    // The hint points at the keyless backend, not only at the signup page.
    assert!(
        v["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("classifier.dev"),
        "{}",
        v["error"]["hint"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_backend_is_a_usage_error() {
    let mut c = common::bin();
    c.env("JEVIFY_BACKEND", "ollama");
    let out = tokio::task::spawn_blocking(move || c.args(["--json", "health"]).output().unwrap())
        .await
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["kind"], "usage");
}

#[tokio::test]
async fn questions_past_the_twenty_dimension_limit_are_split_across_requests() {
    let server = MockServer::start().await;
    // The fake asserts the 20-dimension limit; `.expect(3)` pins the split of 41 questions.
    Mock::given(method("POST"))
        .and(path("/v1/classify"))
        .respond_with(common::FakeClassifier(FakeJev {
            choose: |_, _, _| "NONE".into(),
            noul: |_, _| 0.75,
        }))
        .expect(3)
        .mount(&server)
        .await;
    let mut qs = Questions::new();
    for i in 0..41 {
        qs.insert(format!("q{i:03}"), Question::noul("Is it?"));
    }
    let r = Client::new(&classifier_config(&server))
        .unwrap()
        .ask(&serde_json::json!({ "x": 1 }), &qs)
        .await
        .unwrap();
    // Every answer comes back, from whichever chunk carried it.
    for i in 0..41 {
        assert_eq!(r.noul(&format!("q{i:03}")).unwrap(), 0.75);
    }
}

#[tokio::test]
async fn a_rejected_body_is_an_input_error_with_the_services_own_code() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            serde_json::json!({"error":"input exceeds 32000 characters","code":"input_too_long"}),
        ))
        .mount(&server)
        .await;
    let err = Client::new(&classifier_config(&server))
        .unwrap()
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap_err();
    assert_eq!(err.exit().code(), 6);
    assert_eq!(err.kind(), "api_rejected_request");
    assert!(err.to_string().contains("input_too_long"), "{err}");
}

#[tokio::test]
async fn an_upstream_outage_is_retried_and_ends_as_unavailable() {
    let server = MockServer::start().await;
    // 502 `typesafe` is what classifier.dev answers when Jev itself is down.
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(502)
                .set_body_json(serde_json::json!({"error":"provider failed","code":"typesafe"})),
        )
        .expect(4)
        .mount(&server)
        .await;
    let err = Client::new(&classifier_config(&server))
        .unwrap()
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap_err();
    assert_eq!(err.exit().code(), 4);
}

#[tokio::test]
async fn the_cache_never_crosses_backends() {
    let server = common::mock_classifier(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.42,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut c = classifier_config(&server);
    c.cache_dir = Some(dir.path().to_path_buf());
    let state = serde_json::json!("hello");
    let client = Client::new(&c).unwrap();
    let p = |r: jevify::jev::Response| r.noul("q").unwrap();
    let first = p(client.ask(&state, &one_noul()).await.unwrap());
    assert!((first - 0.42).abs() < 1e-9, "{first}");
    // The second call is served from disk: same answer, one cache hit, no second request.
    assert_eq!(p(client.ask(&state, &one_noul()).await.unwrap()), first);
    assert_eq!(c.meta().cache_hits, 1);
    assert_eq!(c.meta().requests, 1);

    // The same question on the other backend must not read that entry: the TypeSafe mock is
    // not mounted here, so a cache hit would answer 0.42 instead of failing.
    let mut t = common::config(&server);
    t.cache_dir = Some(dir.path().to_path_buf());
    assert!(
        Client::new(&t)
            .unwrap()
            .ask(&state, &one_noul())
            .await
            .is_err()
    );
    assert_eq!(t.meta().cache_hits, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_never_offers_more_options_than_the_service_accepts() {
    // 250 lines is over classifier.dev's 100-label limit, so the tournament must window at 99
    // plus NONE; the fake asserts the limit on every request.
    let server = common::mock_classifier(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    let mut lines: Vec<String> = (0..250).map(|i| format!("file-{i:03}.txt")).collect();
    lines.push("invoice-march.pdf".into());
    let mut c = common::jevify_classifier(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "pick", "the bill"])
            .write_stdin(lines.join("\n"))
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["matches"][0]["text"], "invoice-march.pdf");
    // 251 items in windows of 99 is 3 windows, plus the finals round.
    assert_eq!(v["meta"]["requests"], 4);
}
