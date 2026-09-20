mod common;
use grevi::jev::client::Client;
use grevi::jev::{Question, Questions};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn one_noul() -> Questions {
    let mut q = Questions::new();
    q.insert("q".into(), Question::noul("Is it?"));
    q
}

#[tokio::test]
async fn malformed_decisions_are_protocol_errors_and_never_cached() {
    for answer in [
        serde_json::json!({}),
        serde_json::json!({"q":{"noul":1.1}}),
        serde_json::json!({"q":{"noul":-0.1}}),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"answers":answer})),
            )
            .expect(2)
            .mount(&server)
            .await;
        let mut cfg = common::config(&server);
        let dir = tempfile::tempdir().unwrap();
        cfg.cache_dir = Some(dir.path().to_path_buf());
        let client = Client::new(&cfg).unwrap();
        for _ in 0..2 {
            assert_eq!(
                client
                    .ask(&serde_json::json!("x"), &one_noul())
                    .await
                    .unwrap_err()
                    .kind(),
                "api_protocol"
            );
        }
    }
}

#[tokio::test]
async fn failed_posts_count_every_attempt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).insert_header("retry-after-ms", "0"))
        .expect(4)
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    assert!(
        Client::new(&cfg)
            .unwrap()
            .ask(&serde_json::json!("x"), &one_noul())
            .await
            .is_err()
    );
    assert_eq!(cfg.meta().requests, 4);
}

#[tokio::test]
async fn all_semantic_text_is_redacted_without_changing_option_identity() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, options| options[0].clone(),
        noul: |_, _| 0.8,
    })
    .await;
    let cfg = common::config(&server);
    let mut qs = one_noul();
    qs.insert(
        "q".into(),
        Question::noul_with(
            "condition token=abcdefghijk",
            "secret=abcdefghijk",
            "password=abcdefghijk",
        ),
    );
    qs.insert(
        "pick".into(),
        Question::choice(
            "api_key=abcdefghijk",
            [
                (
                    "opaque_token_id".into(),
                    Some("description token=abcdefghijk".into()),
                ),
                ("NONE".into(), None),
            ]
            .into(),
        ),
    );
    let state = serde_json::json!({"request":"token=abcdefghijk", "filename":"token=abcdefghijk.txt", "nested":["token_expiry_seconds = 3600"]});
    Client::new(&cfg).unwrap().ask(&state, &qs).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(!body.to_string().contains("abcdefghijk"));
    assert!(
        body["questions"]["pick"]["criteria"]
            .get("opaque_token_id")
            .is_some()
    );
    assert_eq!(body["state"]["nested"][0], "token_expiry_seconds = 3600");
    assert_eq!(state["request"], "token=abcdefghijk");
}

#[tokio::test]
async fn invalid_cached_answers_are_rejected() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.8,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = common::config(&server);
    cfg.cache_dir = Some(dir.path().to_path_buf());
    let state = serde_json::json!("x");
    let canonical = serde_json::json!({"decision_contract":2,"endpoint":cfg.base_url,"backend":"typesafe","model":cfg.model,"state":state,"questions":one_noul()});
    let key = grevi::jev::cache::key(&serde_json::to_vec(&canonical).unwrap());
    grevi::jev::cache::DiskCache::new(dir.path().to_path_buf())
        .unwrap()
        .put(
            &key,
            &serde_json::from_value(serde_json::json!({"answers":{}})).unwrap(),
        );
    assert_eq!(
        Client::new(&cfg)
            .unwrap()
            .ask(&state, &one_noul())
            .await
            .unwrap_err()
            .kind(),
        "api_protocol"
    );
    assert_eq!(cfg.meta().cache_hits, 0);
}

#[tokio::test]
async fn cache_is_scoped_to_the_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let mut servers = Vec::new();
    for p in [0.2, 0.8] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"answers":{"q":{"noul":p}}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut cfg = common::config(&server);
        cfg.cache_dir = Some(dir.path().to_path_buf());
        assert_eq!(
            Client::new(&cfg)
                .unwrap()
                .ask(&serde_json::json!("x"), &one_noul())
                .await
                .unwrap()
                .noul("q")
                .unwrap(),
            p
        );
        servers.push(server);
    }
}

#[tokio::test]
async fn equivalent_endpoint_trailing_slashes_share_cache() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.8,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = common::config(&server);
    cfg.cache_dir = Some(dir.path().to_path_buf());
    Client::new(&cfg)
        .unwrap()
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap();
    cfg.base_url.push('/');
    Client::new(&cfg)
        .unwrap()
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap();
    assert_eq!(cfg.meta().requests, 1);
    assert_eq!(cfg.meta().cache_hits, 1);
}

#[tokio::test]
async fn classifier_preflights_all_chunks_before_any_post() {
    let server = MockServer::start().await;
    let mut cfg = common::config(&server);
    cfg.backend = grevi::config::Backend::Classifier;
    let mut qs: Questions = (0..21)
        .map(|i| (format!("q{i:02}"), Question::noul("is it?")))
        .collect();
    qs.insert("q20".into(), Question::noul("x".repeat(4001)));
    assert_eq!(
        Client::new(&cfg)
            .unwrap()
            .ask(&serde_json::json!("x"), &qs)
            .await
            .unwrap_err()
            .exit()
            .code(),
        6
    );
    assert_eq!(cfg.meta().requests, 0);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn retries_429_then_succeeds_and_caches() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"model":"m","answers":{"q":{"noul":0.7}},"usage":{"input_tokens":5}})))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::config(&server);
    c.cache_dir = Some(dir.path().to_path_buf());
    let client = Client::new(&c).unwrap();
    let state = serde_json::json!("hello");
    assert_eq!(
        client
            .ask(&state, &one_noul())
            .await
            .unwrap()
            .noul("q")
            .unwrap(),
        0.7
    );
    // second call is served from disk: the `.expect(1)` above verifies no second POST
    assert_eq!(
        client
            .ask(&state, &one_noul())
            .await
            .unwrap()
            .noul("q")
            .unwrap(),
        0.7
    );
    assert_eq!(c.meta().cache_hits, 1);
}

#[tokio::test]
async fn rejected_key_maps_to_auth_exit() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let c = common::config(&server);
    let err = Client::new(&c)
        .unwrap()
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap_err();
    assert_eq!(err.exit().code(), 5);
}

#[tokio::test]
async fn http_422_is_an_input_error_not_an_outage() {
    let server = MockServer::start().await;
    // The wire shape is FastAPI's `HTTPValidationError` (TypeSafe's OpenAPI schema).
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(422)
                .insert_header("x-typesafe-request-id", "req_test_422")
                .set_body_json(serde_json::json!({"detail":[{"loc":["body","state"],"msg":"too many tokens","type":"value_error"}]})),
        )
        .mount(&server)
        .await;
    let c = common::config(&server);
    let err = Client::new(&c)
        .unwrap()
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap_err();
    assert_eq!(err.exit().code(), 6);
    assert_eq!(err.kind(), "api_rejected_request");
    assert!(err.to_string().contains("HTTP 422"), "{err}");
    assert!(err.to_string().contains("state: too many tokens"), "{err}");
    // The failing response's request id reaches `meta` (and so the error envelope).
    assert_eq!(c.meta().request_id.as_deref(), Some("req_test_422"));
}
