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

#[tokio::test]
async fn missing_dimension_model_reaches_the_output_envelope() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/classify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results":[{"dimensions":{
                "is_0":{"label":"The text clearly satisfies the condition","confidence":0.95},
                "is_1":{"label":"The text clearly satisfies the condition","confidence":0.95,"model":"jev-fake"}
            }}]
        })))
        .mount(&server)
        .await;
    let mut command = common::jevify_classifier(&server);
    let output = tokio::task::spawn_blocking(move || {
        command
            .args(["--json", "is", "first", "second"])
            .write_stdin("evidence")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["meta"]["model"], "unknown, jev-fake");
    assert_eq!(envelope["data"]["statements"][0]["verdict"], "yes");
    assert_eq!(envelope["data"]["statements"][1]["verdict"], "yes");
}

#[tokio::test]
async fn each_batches_by_decision_count_and_returns_every_record_in_order() {
    use futures::TryStreamExt;
    for (count, dimensions, sizes) in [
        (2500, 1, [vec![60; 41], vec![40]].concat()),
        (100, 2, vec![30, 30, 30, 10]),
    ] {
        let server = common::mock_classifier(FakeJev {
            choose: |_, _, _| "NONE".into(),
            noul: |_, state| state.as_str().unwrap().parse::<f64>().unwrap() / 2500.0,
        })
        .await;
        let cfg = classifier_config(&server);
        let client = Client::new(&cfg).unwrap();
        let qs: Questions = (0..dimensions)
            .map(|i| (format!("q{i}"), Question::noul("Is it?")))
            .collect();
        let records: Vec<String> = (0..count).map(|i| i.to_string()).collect();
        let mut batches: Vec<_> = client.ask_each(&records, &qs).try_collect().await.unwrap();
        batches.sort_by_key(|b| b.0);
        assert_eq!(batches.iter().map(|b| b.1.len()).collect::<Vec<_>>(), sizes);
        for (i, response) in batches.into_iter().flat_map(|b| b.1).enumerate() {
            for id in qs.keys() {
                assert!((response.noul(id).unwrap() - i as f64 / 2500.0).abs() < 1e-12);
            }
        }
        let mut actual: Vec<_> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|r| {
                let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
                body["items"].as_array().unwrap().len()
            })
            .collect();
        actual.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(actual, sizes);
    }
}

#[tokio::test]
async fn batch_models_and_redacted_cache_roundtrip() {
    use futures::TryStreamExt;
    let server = common::mock_classifier(
        FakeJev {
            choose: |_, _, _| "NONE".into(),
            noul: |_, _| 0.8,
        }
        .with_model(|instructions| {
            // Noul dimensions append yes/no explanations after the original instruction.
            if instructions.lines().next() == Some("first") {
                "other-model".into()
            } else {
                "jev-fake".into()
            }
        }),
    )
    .await;
    let mut cfg = classifier_config(&server);
    cfg.cache_dir = Some(tempfile::tempdir().unwrap().keep());
    let qs: Questions = [
        ("a".into(), Question::noul("first")),
        ("b".into(), Question::noul("second")),
        ("c".into(), Question::noul("third")),
    ]
    .into();
    let records: Vec<String> = vec!["token=abcdefghijk".into(), "another record".into()];
    for input in [
        &records,
        &records
            .iter()
            .map(|r| jevify::input::redact(r))
            .collect::<Vec<_>>(),
    ] {
        let client = Client::new(&cfg).unwrap();
        let batches: Vec<_> = client.ask_each(input, &qs).try_collect().await.unwrap();
        for response in &batches[0].1 {
            assert_eq!(response.model, "other-model, jev-fake");
            assert!(!response.all_jev());
        }
        assert_eq!(cfg.meta().model.as_deref(), Some("other-model, jev-fake"));
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_eq!(cfg.meta().cache_hits, 1);
}

#[tokio::test]
async fn chunk_models_merge_in_question_order_and_survive_cache() {
    let server = common::mock_classifier(
        FakeJev {
            choose: |_, _, _| "NONE".into(),
            noul: |_, _| 0.8,
        }
        .with_model(|instruction| {
            // Match the original instruction before the appended Noul explanations.
            if instruction.lines().next() == Some("first") {
                "other-model".into()
            } else {
                "jev-fake".into()
            }
        }),
    )
    .await;
    let mut cfg = classifier_config(&server);
    cfg.cache_dir = Some(tempfile::tempdir().unwrap().keep());
    let qs: Questions = (0..45)
        .map(|i| {
            (
                format!("q{i:02}"),
                Question::noul(if i < 20 { "first" } else { "second" }),
            )
        })
        .collect();
    for _ in 0..2 {
        let response = Client::new(&cfg)
            .unwrap()
            .ask(&serde_json::json!("evidence"), &qs)
            .await
            .unwrap();
        assert_eq!(response.model, "other-model, jev-fake");
        assert!(!response.all_jev());
        assert_eq!(cfg.meta().model.as_deref(), Some("other-model, jev-fake"));
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
    assert_eq!(cfg.meta().cache_hits, 1);
}

#[tokio::test]
async fn batches_and_question_chunks_are_concurrent_and_obey_the_semaphore() {
    use futures::TryStreamExt;
    use std::time::Duration;
    use wiremock::Respond;
    for (each, concurrency) in [(false, 3), (true, 3), (true, 1)] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(|req: &wiremock::Request| {
                common::FakeClassifier(FakeJev {
                    choose: |_, _, _| "NONE".into(),
                    noul: |_, _| 0.8,
                })
                .respond(req)
                .set_delay(Duration::from_secs(1))
            })
            .mount(&server)
            .await;
        let mut cfg = classifier_config(&server);
        cfg.concurrency = concurrency;
        let client = Client::new(&cfg).unwrap();
        let qs: Questions = (0..if each { 1 } else { 45 })
            .map(|i| (format!("q{i}"), Question::noul("Is it?")))
            .collect();
        // Three requests either way: 45 questions over 20 dimensions, or 180 records in 60s.
        let records = vec!["record".into(); if each { 180 } else { 2500 }];
        let operation = async {
            if each {
                let _: Vec<_> = client.ask_each(&records, &qs).try_collect().await.unwrap();
            } else {
                client
                    .ask(&serde_json::json!("evidence"), &qs)
                    .await
                    .unwrap();
            }
        };
        tokio::pin!(operation);
        tokio::select! {
            _ = &mut operation => panic!("delayed responses finished before observation"),
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
        assert_eq!(server.received_requests().await.unwrap().len(), concurrency);
        operation.await;
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }
}

#[tokio::test]
async fn quota_decisions_stop_daily_and_long_limits_and_retry_minute() {
    use futures::TryStreamExt;
    for (code, seconds) in [
        ("rate_limit_day", 1),
        ("rate_limit_minute", 61),
        ("rate_limit_hour", 3600),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(FakeJev::quota(code, seconds))
            .expect(1)
            .mount(&server)
            .await;
        let cfg = classifier_config(&server);
        let client = Client::new(&cfg).unwrap();
        let error = client
            .ask_each(&["record".into()], &one_noul())
            .try_collect::<Vec<_>>()
            .await
            .unwrap_err();
        assert_eq!(error.exit().code(), 4);
        assert!(error.to_string().contains(if code == "rate_limit_day" {
            "daily quota of the free backend reached"
        } else {
            code
        }));
    }
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(FakeJev::quota("rate_limit_minute", 1))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(common::FakeClassifier(FakeJev {
            choose: |_, _, _| "NONE".into(),
            noul: |_, _| 0.8,
        }))
        .with_priority(2)
        .expect(1)
        .mount(&server)
        .await;
    let cfg = classifier_config(&server);
    let client = Client::new(&cfg).unwrap();
    let _: Vec<_> = client
        .ask_each(&["record".into()], &one_noul())
        .try_collect()
        .await
        .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn daily_quota_exits_four_after_one_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(FakeJev::quota("rate_limit_day", 3600))
        .expect(1)
        .mount(&server)
        .await;
    let mut cmd = common::jevify_classifier(&server);
    let out = tokio::task::spawn_blocking(move || {
        cmd.args(["--json", "is", "it holds"])
            .write_stdin("record")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(4));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["error"]["message"],
        "API unavailable: daily quota of the free backend reached"
    );
}

#[tokio::test]
async fn fake_vectors_express_ties_and_close_none_on_both_backends() {
    let qs: Questions = [(
        "pick".into(),
        Question::choice(
            "pick",
            [
                ("A".into(), None),
                ("B".into(), None),
                ("NONE".into(), None),
            ]
            .into(),
        ),
    )]
    .into();
    for vector in [
        (|_: &str, _: &serde_json::Value, _: &[String]| vec![0.5, 0.5, 0.0])
            as common::ProbabilityVector,
        |_, _, _| vec![0.5, 0.01, 0.49],
    ] {
        let fake = FakeJev {
            choose: |_, _, _| "A".into(),
            noul: |_, _| 0.8,
        }
        .with_probabilities(vector)
        .with_model(|_| "other-model".into());
        let typed = common::mock(fake.clone()).await;
        let classifier = common::mock_classifier(fake).await;
        for cfg in [common::config(&typed), classifier_config(&classifier)] {
            let response = Client::new(&cfg)
                .unwrap()
                .ask(&serde_json::json!("evidence"), &qs)
                .await
                .unwrap();
            assert_eq!(
                response
                    .probs("pick")
                    .unwrap()
                    .values()
                    .copied()
                    .collect::<Vec<_>>(),
                vector("", &serde_json::Value::Null, &[])
            );
            assert_eq!(response.model, "other-model");
        }
    }
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
