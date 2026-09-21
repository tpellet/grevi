mod common;

use assert_cmd::cargo::CommandCargoExt;
use common::FakeJev;
use serde_json::Value;
use std::io::{BufRead, Read, Write};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use wiremock::{
    Mock, MockServer, Request, Respond, ResponseTemplate,
    matchers::{method, path},
};

fn fake() -> FakeJev {
    FakeJev {
        choose: |_, _, _| unreachable!(),
        noul: |_, state| match state.as_str().unwrap_or_default() {
            "no" => 0.1,
            "unsure" => 0.5,
            _ => 0.9,
        },
    }
}

#[tokio::test]
async fn verdicts_inversion_counts_and_machine_records() {
    let server = common::mock_classifier(fake()).await;
    for (flags, expected, kept) in [
        (vec![], "yes\nunsure\nyes\n", 3),
        (vec!["-v"], "no\nunsure\n", 2),
        (vec!["--strict"], "yes\nyes\n", 2),
        (vec!["-v", "--strict"], "no\n", 1),
        (vec!["-c"], "3\n", 3),
    ] {
        for machine in [false, true] {
            let mut cmd = common::jevify_classifier(&server);
            cmd.args(["filter", "x"]).args(&flags);
            if machine {
                cmd.arg("--json");
            }
            let out = cmd.write_stdin("yes\nno\nunsure\nyes\n").output().unwrap();
            assert_eq!(out.status.code(), Some(0));
            if machine {
                let value: Value = serde_json::from_slice(&out.stdout).unwrap();
                assert_eq!(value["exit_code"], 0);
                assert_eq!(value["data"]["kept"], kept);
                assert_eq!(value["data"]["unsure"], 1);
            } else {
                assert_eq!(out.stdout, expected.as_bytes());
            }
        }
    }
}

fn envelope(out: &std::process::Output, code: i32) -> Value {
    assert_eq!(
        out.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["exit_code"], code);
    assert_eq!(value["command"], "filter");
    value
}

#[test]
fn preflight_errors_need_no_backend_or_saved_input() {
    for (input, flags, code, kind) in [
        (String::new(), vec![], 6, "empty_input"),
        (input(20_001), vec![], 6, "too_many"),
        ("yes\n".into(), vec!["-0", "--para"], 2, "usage"),
    ] {
        let out = common::bin()
            .args(["filter", "x", "--json", "--no-save"])
            .args(flags)
            .write_stdin(input)
            .output()
            .unwrap();
        let value = envelope(&out, code);
        assert_eq!(value["error"]["kind"], kind);
        assert_eq!(value["meta"]["requests"], 0);
    }
}

#[tokio::test]
async fn exit_codes_and_empty_input() {
    let server = common::mock_classifier(fake()).await;
    for (input, strict, code, kept) in [
        ("no\n", false, 1, 0),
        ("unsure\n", false, 3, 1),
        ("unsure\n", true, 3, 0),
        ("no\nunsure\n", true, 1, 0),
        (" \n\t\r\n", false, 1, 0),
    ] {
        let mut cmd = common::jevify_classifier(&server);
        cmd.args(["filter", "x", "--json"]);
        if strict {
            cmd.arg("--strict");
        }
        let out = cmd.write_stdin(input).output().unwrap();
        let value = envelope(&out, code);
        assert_eq!(value["data"]["kept"], kept);
    }
    let out = common::jevify_classifier(&server)
        .args(["filter", "x", "--json"])
        .write_stdin("")
        .output()
        .unwrap();
    assert_eq!(envelope(&out, 6)["error"]["kind"], "empty_input");
}

#[tokio::test]
async fn bytes_splits_order_and_judge_once() {
    let server = common::mock_classifier(fake()).await;
    for (input, flag, expected) in [
        (
            b"\x1b[31myes\x1b[0m\r\n\n\xff\r\n\x1b[31myes\x1b[0m\r\n".as_slice(),
            "",
            b"\x1b[31myes\x1b[0m\r\n\xff\r\n\x1b[31myes\x1b[0m\r\n".as_slice(),
        ),
        (b"yes\0\0\xff\0yes\0", "-0", b"yes\0\xff\0yes\0"),
        (
            b"yes\nline\n\nother\r\n\r\n",
            "--para",
            b"yes\nline\n\nother\r\n\r\n",
        ),
        (b"a\0b\nlast", "", b"a\0b\nlast"),
    ] {
        for machine in [false, true] {
            let mut cmd = common::jevify_classifier(&server);
            cmd.args(["filter", "x"]);
            if !flag.is_empty() {
                cmd.arg(flag);
            }
            if machine {
                cmd.arg("--json");
            }
            let out = cmd.write_stdin(input).output().unwrap();
            assert_eq!(out.status.code(), Some(0));
            if machine {
                let value = envelope(&out, 0);
                let entries = value["data"]["records"].as_array().unwrap();
                let joined: String = entries
                    .iter()
                    .map(|e| e["text"].as_str().unwrap())
                    .collect();
                assert_eq!(joined, String::from_utf8_lossy(expected));
                if input.contains(&255) {
                    assert_eq!(entries[1]["lossy"], true);
                    assert_eq!(entries[1]["ordinal"], 2);
                }
            } else {
                assert_eq!(out.stdout, expected);
            }
        }
    }
    let requests = server.received_requests().await.unwrap();
    let first: Value = serde_json::from_slice(
        &requests
            .iter()
            .find(|r| r.url.path() == "/v1/classify")
            .unwrap()
            .body,
    )
    .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    let dimension = &first["dimensions"]["filter"];
    assert_eq!(
        dimension["labels"],
        serde_json::json!(["the statement holds", "the statement does not hold"])
    );
    assert!(
        dimension["instructions"]
            .as_str()
            .unwrap()
            .contains("judge this one record")
    );
}

#[tokio::test]
async fn batch_boundaries_and_ceiling() {
    for (classifier, count, expected) in [
        (true, 2500, 3),
        (false, 41, 3),
        (true, 20_000, 20),
        (true, 20_001, 0),
    ] {
        let server = if classifier {
            common::mock_classifier(fake()).await
        } else {
            common::mock(fake()).await
        };
        let mut cmd = if classifier {
            common::jevify_classifier(&server)
        } else {
            common::jevify(&server)
        };
        let input: String = (0..count).map(|i| format!("record {i}\n")).collect();
        let out = cmd
            .args(["filter", "x", "--json"])
            .write_stdin(input)
            .output()
            .unwrap();
        let value = envelope(&out, if count > 20_000 { 6 } else { 0 });
        if count > 20_000 {
            assert_eq!(value["error"]["kind"], "too_many");
            assert!(
                value["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("grep or head")
            );
        } else {
            assert_eq!(value["data"]["kept"], count);
            assert!(String::from_utf8_lossy(&out.stderr).contains(&format!(
                "{count} records, {count} distinct, {expected} requests"
            )));
        }
        let requests = server.received_requests().await.unwrap();
        let posts: Vec<_> = requests.iter().filter(|r| r.method == "POST").collect();
        assert_eq!(posts.len(), expected);
        if !classifier {
            for request in posts {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                assert!(body["state"]["items"].as_array().unwrap().len() <= 20);
            }
        }
    }
}

#[tokio::test]
async fn saved_input_status_verbose_failure_and_no_save() {
    let server = common::mock_classifier(fake()).await;
    let root = tempfile::tempdir().unwrap().keep();
    for verbose in [false, true] {
        let mut cmd = common::jevify_classifier(&server);
        cmd.env("JEVIFY_CACHE_DIR", &root)
            .args(["filter", "x", "--json"]);
        if verbose {
            cmd.arg("--verbose");
        }
        let out = cmd.write_stdin("yes\nunsure\nno\n").output().unwrap();
        let value = envelope(&out, 0);
        assert_eq!(value["data"]["complete"], true);
        let saved = value["data"]["saved_input"].as_str().unwrap();
        assert_eq!(std::fs::read(saved).unwrap(), b"yes\nunsure\nno\n");
        let stderr = String::from_utf8_lossy(&out.stderr);
        let summary = format!("jevify filter: kept 2 of 3, 1 unsure, full output: {saved}");
        assert_eq!(stderr.lines().filter(|line| *line == summary).count(), 1);
    }
    assert_eq!(std::fs::read_dir(root.join("outputs")).unwrap().count(), 1);
    let blocked = root.join("file");
    std::fs::write(&blocked, "keep").unwrap();
    for no_save in [false, true] {
        let mut cmd = common::jevify_classifier(&server);
        cmd.env("JEVIFY_CACHE_DIR", &blocked)
            .args(["filter", "x", "--json"]);
        if no_save {
            cmd.arg("--no-save");
        }
        let out = cmd.write_stdin("yes\n").output().unwrap();
        assert_eq!(envelope(&out, 0)["data"]["complete"], false);
        assert!(String::from_utf8_lossy(&out.stderr).contains("full output: not saved ("));
    }
    assert_eq!(std::fs::read(blocked).unwrap(), b"keep");
}

#[tokio::test]
async fn file_excerpts_are_relative_and_secrets_are_withheld() {
    let server = common::mock_classifier(fake()).await;
    let root = tempfile::tempdir().unwrap().keep();
    std::fs::write(root.join("source.rs"), "VISIBLE_EXCERPT").unwrap();
    std::fs::write(root.join(".npmrc"), "PRIVATE_EXCERPT").unwrap();
    for machine in [false, true] {
        let mut cmd = common::jevify_classifier(&server);
        cmd.current_dir(&root)
            .args(["filter", "x", "-0", "--files"]);
        if machine {
            cmd.arg("--json");
        }
        let out = cmd.write_stdin(b"./source.rs\0.npmrc\0").output().unwrap();
        if machine {
            let value = envelope(&out, 0);
            assert_eq!(value["data"]["records"][0]["text"], "./source.rs\0");
            assert_eq!(value["data"]["excerpts_withheld"], 1);
            assert_eq!(value["data"]["kept"], 2);
        } else {
            assert_eq!(out.status.code(), Some(0));
            assert_eq!(out.stdout, b"./source.rs\0.npmrc\0");
            assert!(
                String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .any(|line| line == "jevify filter: excerpts withheld: 1")
            );
        }
    }
    for request in server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method == "POST")
    {
        let body = String::from_utf8_lossy(&request.body);
        assert!(body.contains("VISIBLE_EXCERPT"));
        assert!(!body.contains("PRIVATE_EXCERPT"));
    }
}

#[tokio::test]
async fn fake_predicates_commute_and_model_provenance_is_visible() {
    let predicate = FakeJev {
        choose: |_, _, _| unreachable!(),
        noul: |instructions, state| {
            let text = state.as_str().unwrap();
            let needle = if instructions.ends_with('A') {
                'a'
            } else {
                'b'
            };
            if text.contains(needle) { 0.9 } else { 0.1 }
        },
    };
    let server = common::mock_classifier(predicate).await;
    let mut results = Vec::new();
    for statements in [["A", "B"], ["B", "A"]] {
        let mut input = b"ab\na\nb\nnone\nba\n".to_vec();
        for statement in statements {
            let out = common::jevify_classifier(&server)
                .args(["filter", statement])
                .write_stdin(input)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(0));
            input = out.stdout;
        }
        results.push(input);
    }
    assert_eq!(results[0], b"ab\nba\n");
    assert_eq!(results[0], results[1]);
    let server = common::mock_classifier(
        fake().with_model(|_| "ibm-granite/granite-4.0-h-micro, jev-fake".into()),
    )
    .await;
    let out = common::jevify_classifier(&server)
        .args(["filter", "x", "--json"])
        .write_stdin("yes\n")
        .output()
        .unwrap();
    assert_eq!(
        envelope(&out, 0)["meta"]["model"],
        "ibm-granite/granite-4.0-h-micro, jev-fake"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("answered by ibm-granite/granite-4.0-h-micro, not Jev")
    );
}

#[derive(Clone)]
struct Batches {
    requests: Arc<AtomicUsize>,
    quota_requests: Arc<AtomicUsize>,
    quota: Option<(&'static str, u64)>,
    delay_last: bool,
    delay_first: bool,
}

impl Respond for Batches {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let first = body["items"][0].as_str().unwrap();
        if first == "record 1000" {
            if let Some((code, wait)) = self.quota {
                let attempt = self.quota_requests.fetch_add(1, Ordering::SeqCst);
                if code != "rate_limit_minute" || attempt == 0 {
                    return FakeJev::quota(code, wait);
                }
            }
        }
        let response = common::FakeClassifier(fake()).respond(request);
        if (self.delay_last && first != "record 0") || (self.delay_first && first == "record 0") {
            response.set_delay(Duration::from_secs(3))
        } else {
            response
        }
    }
}

async fn batches(quota: Option<(&'static str, u64)>, delay_last: bool) -> (MockServer, Batches) {
    let server = MockServer::start().await;
    let responder = Batches {
        requests: Arc::new(AtomicUsize::new(0)),
        quota_requests: Arc::new(AtomicUsize::new(0)),
        quota,
        delay_last,
        delay_first: false,
    };
    Mock::given(method("POST"))
        .and(path("/v1/classify"))
        .respond_with(responder.clone())
        .mount(&server)
        .await;
    (server, responder)
}

#[tokio::test]
async fn later_batches_wait_and_duplicates_keep_their_original_positions() {
    let server = MockServer::start().await;
    let responder = Batches {
        requests: Arc::new(AtomicUsize::new(0)),
        quota_requests: Arc::new(AtomicUsize::new(0)),
        quota: None,
        delay_last: false,
        delay_first: true,
    };
    Mock::given(method("POST"))
        .and(path("/v1/classify"))
        .respond_with(responder)
        .mount(&server)
        .await;
    let input = format!("{}record 0\nrecord 1499\n", input(1500));
    for machine in [false, true] {
        let mut cmd = common::jevify_classifier(&server);
        cmd.env("JEVIFY_CONCURRENCY", "4").args(["filter", "x"]);
        if machine {
            cmd.arg("--json");
        }
        let out = cmd.write_stdin(input.clone()).output().unwrap();
        if machine {
            let value = envelope(&out, 0);
            let records = value["data"]["records"].as_array().unwrap();
            let joined: String = records
                .iter()
                .map(|r| r["text"].as_str().unwrap())
                .collect();
            assert_eq!(joined, input);
        } else {
            assert_eq!(out.status.code(), Some(0));
            assert_eq!(out.stdout, input.as_bytes());
        }
    }
}

fn input(count: usize) -> String {
    (0..count).map(|i| format!("record {i}\n")).collect()
}

#[tokio::test]
async fn quota_keeps_a_prefix_and_minute_limit_retries() {
    for (code, wait, expected) in [
        ("rate_limit_day", 1, 4),
        ("rate_limit_hour", 61, 4),
        ("rate_limit_minute", 1, 0),
    ] {
        for machine in [false, true] {
            let (server, responder) = batches(Some((code, wait)), false).await;
            let mut cmd = common::jevify_classifier(&server);
            cmd.env("JEVIFY_CONCURRENCY", "1").args(["filter", "x"]);
            if machine {
                cmd.arg("--json");
            }
            let out = cmd.write_stdin(input(2500)).output().unwrap();
            assert_eq!(
                out.status.code(),
                Some(expected),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            if machine {
                let value = envelope(&out, expected);
                if expected == 4 {
                    assert_eq!(value["error"]["kind"], "api_unavailable");
                    let message = value["error"]["message"].as_str().unwrap();
                    assert!(message.contains("answered 1000 of 2500"));
                    assert!(message.contains(if code == "rate_limit_day" {
                        "daily quota of the free backend reached"
                    } else {
                        "rate_limit_hour"
                    }));
                } else {
                    assert_eq!(value["data"]["kept"], 2500);
                }
            } else {
                assert_eq!(
                    out.stdout,
                    input(if expected == 4 { 1000 } else { 2500 }).as_bytes()
                );
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            if expected == 4 {
                assert_eq!(responder.quota_requests.load(Ordering::SeqCst), 1);
                if !machine {
                    let status = stderr.find("jevify filter: answered 1000 of 2500").unwrap();
                    let error = stderr
                        .find(if code == "rate_limit_day" {
                            "daily quota of the free backend reached"
                        } else {
                            "rate_limit_hour"
                        })
                        .unwrap();
                    assert!(status < error);
                }
            } else {
                assert_eq!(responder.quota_requests.load(Ordering::SeqCst), 2);
            }
        }
    }
}

fn process(server: &MockServer) -> Command {
    let mut cmd = Command::cargo_bin("jevify").unwrap();
    for var in [
        "TYPESAFE_API_KEY",
        "TYPESAFE_API_KEY_FILE",
        "JEVIFY_THRESHOLD",
        "JEVIFY_MODEL",
        "JEVIFY_INVENTORY_FILE",
    ] {
        cmd.env_remove(var);
    }
    cmd.env("JEVIFY_BACKEND", "classifier")
        .env("JEVIFY_BASE_URL", server.uri())
        .env("JEVIFY_CACHE_DIR", tempfile::tempdir().unwrap().keep())
        .env("JEVIFY_NO_CACHE", "1")
        .env("JEVIFY_CONCURRENCY", "1")
        .args(["filter", "x", "--no-save"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

#[tokio::test]
async fn stdout_flows_before_last_answer_and_closed_pipe_cancels() {
    let (server, _) = batches(None, true).await;
    let mut child = process(&server).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(input(2500).as_bytes()).unwrap());
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout);
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        tx.send(first.clone()).unwrap();
        reader.read_to_string(&mut first).unwrap();
        first
    });
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        "record 0\n"
    );
    assert!(child.try_wait().unwrap().is_none());
    assert!(child.wait().unwrap().success());
    writer.join().unwrap();
    assert_eq!(reader.join().unwrap(), input(2500));

    let (server, responder) = batches(None, true).await;
    let mut child = process(&server).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(input(10_074).as_bytes()).unwrap());
    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    for i in 0..3 {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, format!("record {i}\n"));
    }
    drop(reader);
    assert_eq!(child.wait().unwrap().code(), Some(0));
    writer.join().unwrap();
    assert!(responder.requests.load(Ordering::SeqCst) < 11);
}
