mod common;
use common::FakeJev;

#[tokio::test(flavor = "multi_thread")]
async fn oversized_input_abstains_without_judging_either_predicate() {
    let server = common::mock(FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |_, _| 0.95,
    })
    .await;
    for condition in ["contains a refund", "contains no refund"] {
        let mut c = common::jevify(&server);
        let input = format!(
            "{}\nrefund\n{}",
            "ordinary text ".repeat(4000),
            "ordinary text ".repeat(4000)
        );
        let out = tokio::task::spawn_blocking(move || {
            c.args(["--json", "is", condition])
                .write_stdin(input)
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(out.status.code(), Some(3));
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(v["data"]["p"].is_null());
        assert_eq!(v["data"]["verdict"], "unsure");
        assert_eq!(v["data"]["truncated"], true);
        assert!(String::from_utf8_lossy(&out.stderr).contains("not judged"));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn exit_codes_follow_band() {
    let server = common::mock(FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |_, s| {
            if s.to_string().contains("refund") {
                0.9
            } else if s.to_string().contains("maybe") {
                0.5
            } else {
                0.1
            }
        },
    })
    .await;
    for (input, code) in [
        ("I want a refund", 0),
        ("hello there", 1),
        ("maybe something", 3),
    ] {
        let mut c = common::jevify(&server);
        let input = input.to_string();
        let out = tokio::task::spawn_blocking(move || {
            c.args(["is", "asks for a refund"])
                .write_stdin(input)
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(out.status.code(), Some(code));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn json_reports_probability() {
    let server = common::mock(FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |_, _| 0.81,
    })
    .await;
    let mut c = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "is", "x"])
            .write_stdin("text")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["verdict"], "yes");
    assert_eq!(v["data"]["p"], 0.81);
    assert_eq!(v["meta"]["requests"], 1);
}

#[test]
fn band_out_of_range_is_a_usage_error() {
    common::bin()
        .env("TYPESAFE_API_KEY", "k")
        .args(["is", "--band", "0.9", "x"])
        .write_stdin("t")
        .assert()
        .code(2);
}

fn statement_answers() -> FakeJev {
    FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |question, _| {
            if question.contains("\"no\"") {
                0.1
            } else if question.contains("\"unsure\"") {
                0.5
            } else {
                0.9
            }
        },
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn statements_preserve_order_and_aggregate_verdicts_in_one_post() {
    for (statements, code, verdict) in [
        (vec!["yes"], 0, "yes"),
        (vec!["yes", "no"], 1, "no"),
        (vec!["yes", "yes", "yes"], 0, "yes"),
        (vec!["unsure", "no", "yes"], 1, "no"),
        (vec!["no", "unsure", "yes"], 1, "no"),
        (vec!["yes", "unsure", "yes"], 3, "unsure"),
    ] {
        let server = common::mock(statement_answers()).await;
        for machine in [false, true] {
            let mut c = common::jevify(&server);
            if machine {
                c.arg("--json");
            } else {
                c.arg("--verbose");
            }
            c.arg("is").args(&statements);
            let out = tokio::task::spawn_blocking(move || c.write_stdin("text").output().unwrap())
                .await
                .unwrap();
            assert_eq!(out.status.code(), Some(code));
            if machine {
                let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
                assert_eq!(v["data"]["verdict"], verdict);
                assert_eq!(v["meta"]["requests"], 1);
                if statements.len() == 1 {
                    assert_eq!(
                        v["data"],
                        serde_json::json!({"p": 0.9, "verdict": "yes", "truncated": false})
                    );
                } else {
                    let entries = v["data"]["statements"].as_array().unwrap();
                    assert_eq!(entries.len(), statements.len());
                    for (entry, statement) in entries.iter().zip(&statements) {
                        assert_eq!(entry["statement"], *statement);
                        assert_eq!(entry["verdict"], *statement);
                        let p = match *statement {
                            "no" => 0.1,
                            "unsure" => 0.5,
                            _ => 0.9,
                        };
                        assert_eq!(entry["p"], p);
                    }
                }
            } else {
                let expected = if statements.len() == 1 {
                    String::new()
                } else {
                    statements.iter().map(|s| format!("{s}\t{s}\n")).collect()
                };
                assert_eq!(out.stdout, expected.as_bytes());
                assert!(String::from_utf8_lossy(&out.stderr).contains("0.9"));
            }
        }
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        for request in requests {
            assert_eq!(request.method, "POST");
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(
                body["questions"].as_object().unwrap().len(),
                statements.len()
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn twenty_one_statements_use_backend_question_chunks() {
    for classifier in [false, true] {
        let server = if classifier {
            common::mock_classifier(statement_answers()).await
        } else {
            common::mock(statement_answers()).await
        };
        let mut c = if classifier {
            common::jevify_classifier(&server)
        } else {
            common::jevify(&server)
        };
        let statements: Vec<String> = (0..21).map(|i| format!("statement {i}")).collect();
        c.arg("is").args(&statements);
        let out = tokio::task::spawn_blocking(move || c.write_stdin("text").output().unwrap())
            .await
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        let expected: String = statements.iter().map(|s| format!("yes\t{s}\n")).collect();
        assert_eq!(out.stdout, expected.as_bytes());
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), if classifier { 2 } else { 1 });
        let mut sizes: Vec<usize> = requests
            .iter()
            .map(|request| {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                body[if classifier {
                    "dimensions"
                } else {
                    "questions"
                }]
                .as_object()
                .unwrap()
                .len()
            })
            .collect();
        sizes.sort_unstable();
        assert_eq!(sizes, if classifier { vec![1, 20] } else { vec![21] });
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn context_matches_lossy_trimmed_stdin_and_ignores_other_stdin() {
    let server = common::mock(statement_answers()).await;
    let dir = tempfile::tempdir().unwrap().keep();
    let path = dir.join("context");
    let text = b"\x1b[31mtext\x1b[0m  \r\ninvalid \xff\n";
    std::fs::write(&path, text).unwrap();
    let mut results = Vec::new();
    for file in [false, true] {
        let mut c = common::jevify(&server);
        c.args(["--json", "is", "yes", "no"]);
        if file {
            c.arg("--context").arg(&path);
        }
        let out = tokio::task::spawn_blocking(move || {
            c.write_stdin(if file {
                &b"unrelated stdin"[..]
            } else {
                &text[..]
            })
            .output()
            .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(out.status.code(), Some(1));
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        results.push(value["data"].clone());
    }
    assert_eq!(results[0], results[1]);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body, requests[1].body);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_empty_and_oversized_contexts_make_no_requests() {
    let server = common::mock(statement_answers()).await;
    let dir = tempfile::tempdir().unwrap().keep();
    let path = dir.join("context");
    for contents in [None, Some(String::new()), Some("x".repeat(96_001))] {
        if let Some(contents) = &contents {
            std::fs::write(&path, contents).unwrap();
        }
        let mut c = common::jevify(&server);
        c.args(["--json", "is", "yes", "no", "--context"])
            .arg(&path);
        let out = tokio::task::spawn_blocking(move || c.write_stdin("ignored").output().unwrap())
            .await
            .unwrap();
        let oversized = contents.as_ref().is_some_and(|s| s.len() > 96_000);
        assert_eq!(out.status.code(), Some(if oversized { 3 } else { 6 }));
        if oversized {
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(
                v["data"]["reason"],
                "input exceeds the evidence budget; whole input not judged"
            );
            assert_eq!(v["data"]["verdict"], "unsure");
            assert_eq!(v["data"]["statements"].as_array().unwrap().len(), 2);
        }
    }
    let mut c = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["is", "yes"]).write_stdin("").output().unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
}
