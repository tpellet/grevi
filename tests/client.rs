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
