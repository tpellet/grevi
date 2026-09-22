mod common;
use jevify::jev::client::Client;
use jevify::jev::{Question, Questions};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn one_noul() -> Questions {
    let mut q = Questions::new();
    q.insert("q".into(), Question::noul("Is it?"));
    q
}

#[tokio::test]
async fn typesafe_missing_model_survives_requests_batches_and_cache() {
    use futures::TryStreamExt;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let answers: serde_json::Map<String, serde_json::Value> = body["questions"]
                .as_object()
                .unwrap()
                .keys()
                .map(|id| (id.clone(), serde_json::json!({"noul":0.9})))
                .collect();
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"answers":answers}))
        })
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap().keep();
    for _ in 0..2 {
        let mut cfg = common::config(&server);
        cfg.cache_dir = Some(dir.clone());
        assert!(cfg.meta().model.is_none());
        let client = Client::new(&cfg).unwrap();
        let response = client
            .ask(&serde_json::json!("evidence"), &one_noul())
            .await
            .unwrap();
        assert_eq!(response.model, "unknown");
        assert!(!response.all_jev());
        let batches: Vec<_> = client
            .ask_each(&["first".into(), "second".into()], &one_noul())
            .try_collect()
            .await
            .unwrap();
        assert_eq!(batches[0].1.len(), 2);
        for response in &batches[0].1 {
            assert_eq!(response.model, "unknown");
            assert!(!response.all_jev());
        }
        assert_eq!(cfg.meta().model.as_deref(), Some("unknown"));
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    let mut command = common::jevify(&server);
    let output = tokio::task::spawn_blocking(move || {
        command
            .args(["--json", "is", "holds"])
            .write_stdin("evidence")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["meta"]["model"], "unknown");
}

#[tokio::test]
async fn classifier_unknown_chunks_and_record_models_survive_cache() {
    use futures::TryStreamExt;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let results: Vec<_> = body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| {
                    let dimensions: serde_json::Map<String, serde_json::Value> = body["dimensions"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .map(|id| {
                            let mut answer = serde_json::json!({"label":"yes","confidence":0.9});
                            if id == "q20" || item == "known" {
                                answer["model"] = "jev-fake".into();
                            }
                            (id.clone(), answer)
                        })
                        .collect();
                    serde_json::json!({"dimensions":dimensions})
                })
                .collect();
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"results":results}))
        })
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap().keep();
    let qs: Questions = (0..21)
        .map(|i| (format!("q{i:02}"), Question::noul("holds")))
        .collect();
    for cached in [false, true] {
        let mut cfg = common::config(&server);
        cfg.backend = jevify::config::Backend::Classifier;
        cfg.cache_dir = Some(dir.clone());
        let client = Client::new(&cfg).unwrap();
        let response = client
            .ask(&serde_json::json!("evidence"), &qs)
            .await
            .unwrap();
        assert_eq!(response.model, "unknown, jev-fake");
        assert!(!response.all_jev());
        let batches: Vec<_> = client
            .ask_each(&["unknown".into(), "known".into()], &one_noul())
            .try_collect()
            .await
            .unwrap();
        assert_eq!(batches[0].1[0].model, "unknown");
        assert!(!batches[0].1[0].all_jev());
        assert_eq!(batches[0].1[1].model, "jev-fake");
        assert!(batches[0].1[1].all_jev());
        assert_eq!(cfg.meta().model.as_deref(), Some("unknown, jev-fake"));
        assert_eq!(cfg.meta().cache_hits, if cached { 2 } else { 0 });
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn typesafe_batches_name_records_and_cache_redacted_input() {
    use futures::TryStreamExt;
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |instructions, state| {
            let id: usize = instructions
                .strip_prefix("Judge record id ")
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            assert!(instructions.contains("alone."));
            let record = &state["items"][id];
            assert_eq!(record["id"], id);
            record["text"]
                .as_str()
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap()
                / 100.0
        },
    })
    .await;
    let mut cfg = common::config(&server);
    cfg.cache_dir = Some(tempfile::tempdir().unwrap().keep());
    let qs = one_noul();
    let records: Vec<String> = (0..45).map(|i| format!("{i} token=abcdefghijk")).collect();
    let client = Client::new(&cfg).unwrap();
    let mut batches: Vec<_> = client.ask_each(&records, &qs).try_collect().await.unwrap();
    batches.sort_by_key(|b| b.0);
    assert_eq!(
        batches.iter().map(|b| b.1.len()).collect::<Vec<_>>(),
        [20, 20, 5]
    );
    for (i, response) in batches.into_iter().flat_map(|b| b.1).enumerate() {
        assert_eq!(response.noul("q").unwrap(), i as f64 / 100.0);
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    for request in requests {
        assert!(!String::from_utf8_lossy(&request.body).contains("abcdefghijk"));
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(
            body["questions"].as_object().unwrap().len(),
            body["state"]["items"].as_array().unwrap().len()
        );
    }
    let redacted: Vec<String> = records.iter().map(|r| jevify::input::redact(r)).collect();
    let second_client = Client::new(&cfg).unwrap();
    let _: Vec<_> = second_client
        .ask_each(&redacted, &qs)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
    assert_eq!(cfg.meta().cache_hits, 3);
}

#[tokio::test]
async fn previous_cache_contract_is_bypassed_and_current_contract_hits() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.8,
    })
    .await;
    let mut cfg = common::config(&server);
    let dir = tempfile::tempdir().unwrap().keep();
    cfg.cache_dir = Some(dir.clone());
    let state = serde_json::json!("evidence");
    let qs = one_noul();
    let canonical = serde_json::json!({"decision_contract":3,"endpoint":cfg.base_url,"backend":"typesafe","model":cfg.model,"state":state,"questions":qs});
    let key = jevify::jev::cache::key(&serde_json::to_vec(&canonical).unwrap());
    jevify::jev::cache::DiskCache::new(dir).unwrap().put(
        &key,
        &serde_json::from_value(
            serde_json::json!({"model":"old-model","answers":{"q":{"noul":0.1}}}),
        )
        .unwrap(),
    );
    for _ in 0..2 {
        let response = Client::new(&cfg).unwrap().ask(&state, &qs).await.unwrap();
        assert_eq!(response.noul("q").unwrap(), 0.8);
        assert_eq!(response.model, "jev-fake");
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_eq!(cfg.meta().cache_hits, 1);
}

#[tokio::test]
async fn command_helpers_isolate_saved_inputs_in_retained_directories() {
    let server = MockServer::start().await;
    let mut dirs = Vec::new();
    for command in [
        common::jevify(&server),
        common::jevify(&server),
        common::jevify_classifier(&server),
    ] {
        let env: Vec<_> = command.get_envs().collect();
        let dir = env
            .iter()
            .find(|(key, _)| *key == "JEVIFY_CACHE_DIR")
            .unwrap()
            .1
            .unwrap();
        let dir = std::path::PathBuf::from(dir);
        assert!(dir.is_dir());
        assert!(!dirs.contains(&dir));
        dirs.push(dir);
        assert_eq!(
            env.iter()
                .find(|(key, _)| *key == "JEVIFY_NO_CACHE")
                .unwrap()
                .1
                .unwrap(),
            "1"
        );
    }
}

#[tokio::test]
async fn empty_and_single_record_batches_work_on_both_backends() {
    use futures::TryStreamExt;
    for backend in [
        jevify::config::Backend::Classifier,
        jevify::config::Backend::Typesafe,
    ] {
        let fake = common::FakeJev {
            choose: |_, _, _| "NONE".into(),
            noul: |_, _| 0.8,
        };
        let server = match backend {
            jevify::config::Backend::Classifier => common::mock_classifier(fake).await,
            jevify::config::Backend::Typesafe => common::mock(fake).await,
        };
        let mut cfg = common::config(&server);
        cfg.backend = backend;
        let client = Client::new(&cfg).unwrap();
        let qs = one_noul();
        let empty: Vec<_> = client.ask_each(&[], &qs).try_collect().await.unwrap();
        assert!(empty.is_empty());
        assert!(server.received_requests().await.unwrap().is_empty());
        let records = vec!["😀".repeat(20_000)];
        let batches: Vec<_> = client.ask_each(&records, &qs).try_collect().await.unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].0, 0);
        assert_eq!(batches[0].1.len(), 1);
        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        let item = match backend {
            jevify::config::Backend::Classifier => &body["items"][0],
            jevify::config::Backend::Typesafe => &body["state"]["items"][0]["text"],
        };
        assert_eq!(item.as_str().unwrap().encode_utf16().count(), 32_000);
        assert!(
            client
                .ask_each(&["x".into()], &Questions::new())
                .try_collect::<Vec<_>>()
                .await
                .is_err()
        );
    }
}

#[test]
fn configured_endpoints_respect_key_selected_backend() {
    for (key, endpoint, code) in [
        (true, "https://API.TypeSafe.AI", 0),
        (true, "https://classifier.dev", 2),
        (false, "https://api.typesafe.ai", 2),
        (false, "https://classifier.dev:443", 0),
        (true, "http://api.typesafe.ai", 2),
        (false, "https://unknown.example", 2),
        (true, "https://user:private-password@api.typesafe.ai", 2),
        (true, "", 0),
        (false, "   ", 0),
        (false, "not a URL", 2),
    ] {
        let mut cmd = common::bin();
        if key {
            cmd.env("TYPESAFE_API_KEY", "test-key");
        }
        let out = cmd
            .env("JEVIFY_BASE_URL", endpoint)
            .args(["capabilities", "--json"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(code), "{endpoint}");
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["exit_code"], code);
        if code == 2 {
            assert_eq!(value["error"]["kind"], "usage");
        }
        assert!(!String::from_utf8_lossy(&out.stdout).contains("private-password"));
        assert!(!String::from_utf8_lossy(&out.stderr).contains("private-password"));
    }
}

#[tokio::test]
async fn invalid_endpoints_fail_before_sending_anything() {
    let server = MockServer::start().await;
    let mut cfg = common::config(&server);
    for endpoint in [
        "http://api.typesafe.ai".to_string(),
        "https://unknown.example".to_string(),
        "https://classifier.dev".to_string(),
        "https://user:pw@api.typesafe.ai".to_string(),
        server.uri().replace("http://", "http://user:pw@"),
    ] {
        cfg.base_url = endpoint;
        let err = Client::new(&cfg).err().expect("endpoint must be refused");
        assert_eq!(err.exit().code(), 2);
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn redirects_never_receive_key_or_evidence() {
    let target = MockServer::start().await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", target.uri()))
        .expect(1)
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    assert!(
        Client::new(&cfg)
            .unwrap()
            .ask(&serde_json::json!("evidence"), &one_noul())
            .await
            .is_err()
    );
    assert!(target.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn localhost_endpoint_is_accepted() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.8,
    })
    .await;
    let mut cfg = common::config(&server);
    cfg.base_url = server.uri().replace("127.0.0.1", "localhost");
    Client::new(&cfg)
        .unwrap()
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
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
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(
        meta["telemetry"]["inference_posts"],
        serde_json::json!({
            "attempted":4,"succeeded":0,"failed":4,"cancelled":0,"in_flight":0
        })
    );
    assert_eq!(meta["telemetry"]["retry_sends"], 3);
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["unknown_attempts"],
        4
    );
    assert_eq!(meta["telemetry"]["semantic_calls"]["failed"], 1);
    assert_eq!(meta["input_tokens"], serde_json::Value::Null);
    assert_eq!(meta["cost_usd"], serde_json::Value::Null);
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
    let canonical = serde_json::json!({"decision_contract":4,"endpoint":cfg.base_url,"backend":"typesafe","model":cfg.model,"state":state,"questions":one_noul()});
    let key = jevify::jev::cache::key(&serde_json::to_vec(&canonical).unwrap());
    jevify::jev::cache::DiskCache::new(dir.path().to_path_buf())
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
    cfg.backend = jevify::config::Backend::Classifier;
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
    let meta = serde_json::to_value(c.meta()).unwrap();
    assert_eq!(meta["telemetry"]["inference_posts"]["attempted"], 2);
    assert_eq!(meta["telemetry"]["inference_posts"]["succeeded"], 1);
    assert_eq!(meta["telemetry"]["semantic_calls"]["succeeded"], 2);
    assert_eq!(meta["telemetry"]["semantic_questions"], 2);
    assert_eq!(meta["telemetry"]["retry_sends"], 1);
    assert!(meta["telemetry"]["retry_sleep_ms"].as_u64().unwrap() >= 250);
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["reported_subtotal"],
        5
    );
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["unknown_attempts"],
        1
    );
}

#[tokio::test]
async fn usage_survives_semantic_failure_and_missing_usage_stays_unknown() {
    for (body, input, output, reported) in [
        (
            serde_json::json!({"answers":{},"usage":{"input_tokens":7,"output_tokens":3}}),
            7,
            3,
            1,
        ),
        (serde_json::json!({"answers":{}}), 0, 0, 0),
        (
            serde_json::json!({"usage":{"input_tokens":7,"output_tokens":3}}),
            7,
            3,
            1,
        ),
        (
            serde_json::json!({"answers":{},"usage":{"input_tokens":0,"output_tokens":0}}),
            0,
            0,
            1,
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
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
        let meta = serde_json::to_value(cfg.meta()).unwrap();
        assert_eq!(meta["telemetry"]["inference_posts"]["succeeded"], 1);
        assert_eq!(meta["telemetry"]["semantic_calls"]["failed"], 1);
        for (field, subtotal) in [("input_tokens", input), ("output_tokens", output)] {
            assert_eq!(
                meta["telemetry"]["usage"][field]["reported_subtotal"],
                subtotal
            );
            assert_eq!(
                meta["telemetry"]["usage"][field]["reported_attempts"],
                reported
            );
            assert_eq!(
                meta["telemetry"]["usage"][field]["unknown_attempts"],
                1 - reported
            );
            assert_eq!(meta["telemetry"]["usage"][field]["complete"], reported == 1);
        }
    }
}

#[tokio::test]
async fn cancellation_preserves_attempt_conservation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(2)))
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    let client = Client::new(&cfg).unwrap();
    let state = serde_json::json!("x");
    let qs = one_noul();
    let mut ask = Box::pin(client.ask(&state, &qs));
    tokio::select! {
        _ = &mut ask => panic!("response must remain pending"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(30)) => {}
    }
    let pending = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(pending["telemetry"]["inference_posts"]["in_flight"], 1);
    drop(ask);
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    for field in ["inference_posts", "semantic_calls"] {
        assert_eq!(
            meta["telemetry"][field],
            serde_json::json!({
                "attempted":1,"succeeded":0,"failed":0,"cancelled":1,"in_flight":0
            })
        );
    }
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["unknown_attempts"],
        1
    );
}

#[tokio::test]
async fn health_and_prewarm_have_separate_attempt_counters() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.8,
    })
    .await;
    let cfg = common::config(&server);
    jevify::cmd::agent::health(&cfg).await.unwrap();
    Client::new(&cfg).unwrap().prewarm();
    for _ in 0..100 {
        let meta = serde_json::to_value(cfg.meta()).unwrap();
        if meta["telemetry"]["prewarm_gets"]["succeeded"] == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    for field in ["health_gets", "prewarm_gets"] {
        assert_eq!(
            meta["telemetry"][field],
            serde_json::json!({
                "attempted":1,"succeeded":1,"failed":0,"cancelled":0,"in_flight":0
            })
        );
    }
    assert_eq!(meta["requests"], 0);
}

#[tokio::test]
async fn classifier_partial_failure_retains_each_physical_attempt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(common::FakeClassifier(common::FakeJev {
            choose: |_, _, _| "NONE".into(),
            noul: |_, _| 0.8,
        }))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        // Let the successful chunk finish before the error cancels its siblings.
        .respond_with(ResponseTemplate::new(401).set_delay(std::time::Duration::from_millis(100)))
        .with_priority(2)
        .mount(&server)
        .await;
    let mut cfg = common::config(&server);
    cfg.backend = jevify::config::Backend::Classifier;
    cfg.price_per_mtok = 0.0;
    let qs = (0..21)
        .map(|i| (format!("q{i:02}"), Question::noul("is it?")))
        .collect();
    assert!(
        Client::new(&cfg)
            .unwrap()
            .ask(&serde_json::json!("x"), &qs)
            .await
            .is_err()
    );
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(
        meta["telemetry"]["inference_posts"],
        serde_json::json!({
            "attempted":2,"succeeded":1,"failed":1,"cancelled":0,"in_flight":0
        })
    );
    assert_eq!(meta["telemetry"]["semantic_questions"], 21);
    assert_eq!(meta["telemetry"]["semantic_calls"]["failed"], 1);
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["unknown_attempts"],
        2
    );
    assert_eq!(meta["input_tokens"], serde_json::Value::Null);
    assert_eq!(meta["cost_usd"], 0.0);
    assert_eq!(meta["telemetry"]["cost_estimate"]["basis"], "free_service");
}

#[tokio::test]
async fn malformed_json_counts_transport_success_with_unknown_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
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
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(meta["telemetry"]["inference_posts"]["succeeded"], 1);
    assert_eq!(meta["telemetry"]["semantic_calls"]["failed"], 1);
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["complete"],
        false
    );
    assert_eq!(meta["input_tokens"], serde_json::Value::Null);
}

#[tokio::test]
async fn cancelled_retry_sleep_records_elapsed_time_without_another_send() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).insert_header("retry-after-ms", "1000"))
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    let client = Client::new(&cfg).unwrap();
    let state = serde_json::json!("x");
    let qs = one_noul();
    let mut ask = Box::pin(client.ask(&state, &qs));
    tokio::select! {
        _ = &mut ask => panic!("retry must remain pending"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
    }
    drop(ask);
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(
        meta["telemetry"]["inference_posts"],
        serde_json::json!({
            "attempted":1,"succeeded":0,"failed":1,"cancelled":0,"in_flight":0
        })
    );
    assert_eq!(meta["telemetry"]["retry_sends"], 0);
    let slept = meta["telemetry"]["retry_sleep_ms"].as_u64().unwrap();
    assert!(
        (1..1000).contains(&slept),
        "actual interrupted sleep: {slept}"
    );
    assert_eq!(meta["telemetry"]["semantic_calls"]["cancelled"], 1);
}

#[tokio::test]
async fn complete_input_usage_prices_only_the_reported_input_tokens() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.8,
    })
    .await;
    let cfg = common::config(&server);
    Client::new(&cfg)
        .unwrap()
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap();
    let meta = cfg.meta();
    assert_eq!(meta.input_tokens, Some(100));
    assert!((meta.cost_usd.unwrap() - 0.0000042).abs() < 1e-12);
    let value = serde_json::to_value(meta).unwrap();
    assert_eq!(
        value["telemetry"]["usage"]["output_tokens"]["reported_subtotal"],
        10
    );
    assert_eq!(
        value["telemetry"]["cost_estimate"]["input_price_per_mtok"],
        0.042
    );
    assert_eq!(
        value["telemetry"]["cost_estimate"]["basis"],
        "configured_input_token_price"
    );
}

#[tokio::test]
async fn token_subtotal_overflow_remains_unknown_without_wrapping() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "answers":{"q":{"noul":0.8}}, "usage":{"input_tokens":u64::MAX,"output_tokens":0}
        })))
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    let client = Client::new(&cfg).unwrap();
    for _ in 0..2 {
        client
            .ask(&serde_json::json!("x"), &one_noul())
            .await
            .unwrap();
    }
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(meta["input_tokens"], serde_json::Value::Null);
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["reported_subtotal"],
        u64::MAX
    );
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["reported_attempts"],
        1
    );
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["unknown_attempts"],
        1
    );
    assert_eq!(meta["telemetry"]["inference_posts"]["succeeded"], 2);
}

#[tokio::test]
async fn failed_health_and_prewarm_do_not_become_inference_attempts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    assert!(jevify::cmd::agent::health(&cfg).await.is_err());
    Client::new(&cfg).unwrap().prewarm();
    for _ in 0..100 {
        if serde_json::to_value(cfg.meta()).unwrap()["telemetry"]["prewarm_gets"]["failed"] == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    for field in ["health_gets", "prewarm_gets"] {
        assert_eq!(
            meta["telemetry"][field],
            serde_json::json!({
                "attempted":1,"succeeded":0,"failed":1,"cancelled":0,"in_flight":0
            })
        );
    }
    assert_eq!(meta["requests"], 0);
    assert_eq!(meta["input_tokens"], 0);
}

#[tokio::test]
async fn cancelling_while_waiting_for_a_permit_does_not_start_an_attempt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(2)))
        .mount(&server)
        .await;
    let mut cfg = common::config(&server);
    cfg.concurrency = 1;
    let client = Client::new(&cfg).unwrap();
    let state = serde_json::json!("x");
    let qs = one_noul();
    let mut first = Box::pin(client.ask(&state, &qs));
    let mut second = Box::pin(client.ask(&state, &qs));
    tokio::select! {
        _ = &mut first => panic!("first request must remain pending"),
        _ = &mut second => panic!("second request must remain pending"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(30)) => {}
    }
    drop(first);
    drop(second);
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(
        meta["telemetry"]["inference_posts"],
        serde_json::json!({
            "attempted":1,"succeeded":0,"failed":0,"cancelled":1,"in_flight":0
        })
    );
    assert_eq!(meta["telemetry"]["semantic_calls"]["cancelled"], 2);
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
async fn incomplete_error_body_preserves_the_http_error_classification() {
    use std::io::{Read, Write};
    let server = MockServer::start().await;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut cfg = common::config(&server);
    cfg.base_url = format!("http://{}", listener.local_addr().unwrap());
    let responder = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let headers = String::from_utf8(request).unwrap();
        let length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap();
        stream.read_exact(&mut vec![0; length]).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 100\r\nConnection: close\r\n\r\nx",
            )
            .unwrap();
    });
    let result = Client::new(&cfg)
        .unwrap()
        .ask(&serde_json::json!("x"), &one_noul())
        .await;
    responder.join().unwrap();
    assert_eq!(result.unwrap_err().exit().code(), 5);
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(meta["telemetry"]["inference_posts"]["failed"], 1);
    assert_eq!(
        meta["telemetry"]["usage"]["input_tokens"]["unknown_attempts"],
        1
    );
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

/// Answers 429 `Retry-After: 1` for the first `limited` requests, then a valid decision, and
/// records when each request arrived.
struct TimedLimiter {
    arrivals: std::sync::Arc<std::sync::Mutex<Vec<std::time::Instant>>>,
    limited: usize,
}

impl wiremock::Respond for TimedLimiter {
    fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
        let mut arrivals = self.arrivals.lock().unwrap();
        arrivals.push(std::time::Instant::now());
        if arrivals.len() <= self.limited {
            ResponseTemplate::new(429).insert_header("Retry-After", "1")
        } else {
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"model":"m","answers":{"q":{"noul":0.7}},"usage":{"input_tokens":5,"output_tokens":1}}),
            )
        }
    }
}

#[tokio::test]
async fn no_request_is_sent_before_retry_after_ends() {
    let arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(TimedLimiter {
            arrivals: arrivals.clone(),
            limited: 2,
        })
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    let client = Client::new(&cfg).unwrap();
    let response = client
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap();
    assert_eq!(response.noul("q").unwrap(), 0.7);
    let arrivals = arrivals.lock().unwrap();
    assert_eq!(arrivals.len(), 3);
    // Two named waits of one second each: the third request comes no earlier than 2 s after
    // the first, and each retry waits its own second.
    assert!(
        arrivals[2].duration_since(arrivals[0]) >= std::time::Duration::from_secs(2),
        "{:?}",
        arrivals[2].duration_since(arrivals[0])
    );
    assert!(arrivals[1].duration_since(arrivals[0]) >= std::time::Duration::from_secs(1));
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(
        meta["usage"],
        serde_json::json!({
            "attempted": 3, "succeeded": 1,
            "waited": {"count": 2, "total_ms": meta["telemetry"]["retry_sleep_ms"]},
            "cache_hits": 0,
            // The two refused attempts reported no usage: unknown, not zero.
            "tokens": {"input": null, "output": null}
        })
    );
    assert!(meta["usage"]["waited"]["total_ms"].as_u64().unwrap() >= 2000);
}

#[tokio::test]
async fn a_deadline_shorter_than_the_wait_ends_without_another_request() {
    let arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(TimedLimiter {
            arrivals: arrivals.clone(),
            limited: usize::MAX,
        })
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    let client = Client::new(&cfg)
        .unwrap()
        .with_budget(std::time::Duration::from_millis(900));
    let start = std::time::Instant::now();
    let error = client
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap_err();
    assert_eq!(error.exit().code(), 4);
    assert!(error.to_string().contains("deadline of 0.9 s"), "{error}");
    assert!(error.to_string().contains("JEVIFY_DEADLINE"), "{error}");
    // The 1 s wait would end past the deadline: it is not started, nothing else is sent, and
    // the verb ends as soon as the refusal is read, before the deadline itself.
    assert!(
        start.elapsed() < std::time::Duration::from_millis(900),
        "{:?}",
        start.elapsed()
    );
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(arrivals.lock().unwrap().len(), 1, "{error}: {meta}");
    assert_eq!(meta["usage"]["attempted"], 1);
    assert_eq!(
        meta["usage"]["waited"],
        serde_json::json!({"count": 0, "total_ms": 0})
    );
}

#[tokio::test]
async fn the_deadline_cancels_a_request_in_flight() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(3)))
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    let client = Client::new(&cfg)
        .unwrap()
        .with_budget(std::time::Duration::from_millis(200));
    let start = std::time::Instant::now();
    let error = client
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap_err();
    assert_eq!(error.exit().code(), 4);
    assert!(error.to_string().contains("deadline of 0.2 s"), "{error}");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(meta["telemetry"]["inference_posts"]["cancelled"], 1);
    assert_eq!(meta["usage"]["attempted"], 1);
    assert_eq!(meta["usage"]["succeeded"], 0);
}

#[tokio::test]
async fn daily_quota_is_one_request_and_exit_4_under_any_deadline() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(common::FakeJev::quota("rate_limit_day", 1))
        .mount(&server)
        .await;
    let cfg = common::config(&server);
    let client = Client::new(&cfg).unwrap();
    let error = client
        .ask(&serde_json::json!("x"), &one_noul())
        .await
        .unwrap_err();
    assert_eq!(error.exit().code(), 4);
    assert_eq!(
        error.to_string(),
        "API unavailable: daily quota of the free backend reached"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn usage_counts_a_cache_hit_as_a_hit_and_measured_tokens_as_numbers() {
    let server = common::mock(common::FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.8,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = common::config(&server);
    cfg.cache_dir = Some(dir.path().to_path_buf());
    let client = Client::new(&cfg).unwrap();
    let state = serde_json::json!("hello");
    client.ask(&state, &one_noul()).await.unwrap();
    client.ask(&state, &one_noul()).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    let meta = serde_json::to_value(cfg.meta()).unwrap();
    assert_eq!(meta["usage"]["attempted"], 1);
    assert_eq!(meta["usage"]["succeeded"], 1);
    assert_eq!(meta["usage"]["cache_hits"], 1);
    assert_eq!(
        meta["usage"]["waited"],
        serde_json::json!({"count": 0, "total_ms": 0})
    );
    assert!(meta["usage"]["tokens"]["input"].is_u64(), "{meta}");
    assert!(meta["usage"]["tokens"]["output"].is_u64(), "{meta}");
    assert_eq!(meta["usage"]["tokens"]["input"], meta["input_tokens"]);
}
