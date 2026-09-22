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

/// The record under judgment, on either wire: classifier.dev sends the record as the state,
/// TypeSafe sends every record of the batch and names the record in the instructions.
fn record_text(instructions: &str, state: &Value) -> String {
    if let Some(text) = state.as_str() {
        return text.to_string();
    }
    let id: usize = instructions
        .strip_prefix("Judge record id ")
        .and_then(|rest| rest.split(' ').next())
        .and_then(|id| id.parse().ok())
        .expect("TypeSafe questions name their record");
    state["items"][id]["text"].as_str().unwrap().to_string()
}

/// A fake that answers from the record's text: a word names the winning label, `unsure`
/// splits the probability between the first two labels, `close` puts the best label under the
/// winner ratio, `nothing` gives NONE the win, `tie` ties NONE with the best label.
fn fake() -> common::ConfiguredFake {
    FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| unreachable!(),
    }
    .with_probabilities(|instructions, state, labels| {
        let text = record_text(instructions, state);
        let n = labels.len();
        let none = labels.iter().position(|l| l == "NONE").unwrap();
        let first = usize::from(none == 0);
        let second = if none == 1 { 2 } else { first + 1 };
        let mut vector = vec![0.0; n];
        let rest = |v: &mut [f64], top: usize, p: f64| {
            let share = (1.0 - p) / (n - 1) as f64;
            for (i, slot) in v.iter_mut().enumerate() {
                *slot = if i == top { p } else { share };
            }
        };
        if text.contains("unsure") {
            vector[first] = 0.45;
            vector[second] = 0.45;
            vector[none] = 0.1;
        } else if text.contains("close") {
            vector[first] = 0.5;
            vector[second] = 0.4;
            vector[none] = 0.1;
        } else if text.contains("nothing") {
            rest(&mut vector, none, 0.7);
        } else if text.contains("tie") {
            vector[first] = 0.4;
            vector[none] = 0.4;
            vector[second] = 0.2;
        } else if let Some(hit) = labels
            .iter()
            .position(|l| l != "NONE" && text.contains(l.as_str()))
        {
            rest(&mut vector, hit, 0.8);
        } else {
            rest(&mut vector, first, 0.8);
        }
        vector
    })
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
    assert_eq!(value["command"], "label");
    value
}

/// `cut -f2-`: the bytes after the first tab of each record of the output.
fn cut_labels(output: &[u8], terminator: u8) -> (Vec<String>, Vec<u8>) {
    let mut labels = Vec::new();
    let mut rest = Vec::new();
    for record in output.split_inclusive(|b| *b == terminator) {
        let tab = record.iter().position(|b| *b == b'\t').unwrap();
        labels.push(String::from_utf8(record[..tab].to_vec()).unwrap());
        rest.extend_from_slice(&record[tab + 1..]);
    }
    (labels, rest)
}

#[tokio::test]
async fn histogram_and_the_way_back_for_lines() {
    let server = common::mock_classifier(fake()).await;
    let input = b"a bug report\r\n\n\x1b[31ma feature\x1b[0m\twith a tab\n \t\nanother bug\nquestion \xff here\nbug again\n";
    let without_blank_lines =
        b"a bug report\r\n\x1b[31ma feature\x1b[0m\twith a tab\nanother bug\nquestion \xff here\nbug again\n";
    let out = common::jevify_classifier(&server)
        .args(["label", "bug,feature,question"])
        .write_stdin(input.as_slice())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (labels, rest) = cut_labels(&out.stdout, b'\n');
    assert_eq!(rest, without_blank_lines);
    assert_eq!(labels, ["bug", "feature", "bug", "question", "bug"]);
    let mut histogram: Vec<(usize, &str)> = ["bug", "feature", "question"]
        .iter()
        .map(|l| (labels.iter().filter(|x| x == l).count(), *l))
        .collect();
    histogram.sort();
    assert_eq!(histogram, [(1, "feature"), (1, "question"), (3, "bug")]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("jevify label: 5 records, 5 distinct, 1 requests"));
    assert!(stderr.contains("jevify label: labelled 5 of 5, 0 unsure"));

    let out = common::jevify_classifier(&server)
        .args(["label", "bug,feature,question", "--json"])
        .write_stdin(input.as_slice())
        .output()
        .unwrap();
    let value = envelope(&out, 0);
    assert_eq!(value["data"]["labelled"], 5);
    assert_eq!(value["data"]["total"], 5);
    assert_eq!(value["data"]["unsure"], 0);
    assert_eq!(value["data"]["complete"], true);
    let entries = value["data"]["records"].as_array().unwrap();
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[3]["label"], "question");
    assert_eq!(entries[3]["text"], "question \u{fffd} here\n");
    assert_eq!(entries[3]["lossy"], true);
    assert_eq!(entries[3]["ordinal"], 4);
    assert_eq!(entries[3]["p"], 0.8);
    assert!(entries[0].get("lossy").is_none());
    assert_eq!(entries[0]["text"], "a bug report\r\n");
    let requests = server.received_requests().await.unwrap();
    let first: Value = serde_json::from_slice(
        &requests
            .iter()
            .find(|r| r.url.path() == "/v1/classify")
            .unwrap()
            .body,
    )
    .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 5);
    assert_eq!(first["items"][1], "a feature\twith a tab");
    let dimension = &first["dimensions"]["label"];
    assert_eq!(
        dimension["labels"],
        serde_json::json!(["NONE", "bug", "feature", "question"])
    );
    assert!(
        dimension["instructions"]
            .as_str()
            .unwrap()
            .contains("NONE —")
    );
}

#[tokio::test]
async fn paragraphs_and_nul_records_follow_the_first_tab_unchanged() {
    let server = common::mock_classifier(fake()).await;
    let paragraph = b"a bug\nsecond\tline\r\n";
    let input = format!(
        "{}\nfeature one\n\n\nquestion\n",
        String::from_utf8_lossy(paragraph)
    );
    let out = common::jevify_classifier(&server)
        .args(["label", "bug,feature,question", "--para"])
        .write_stdin(input.as_bytes())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let tab = out.stdout.iter().position(|b| *b == b'\t').unwrap();
    assert_eq!(&out.stdout[..tab], b"bug");
    let first_record = paragraph.len() + 1;
    assert_eq!(
        &out.stdout[tab + 1..tab + 1 + first_record],
        b"a bug\nsecond\tline\r\n\n"
    );
    assert_eq!(
        &out.stdout[tab + 1 + first_record..],
        b"feature\tfeature one\n\nquestion\tquestion\n"
    );

    let input = b"bug\tone\0\0feature\0\xff question\0tie\0";
    for machine in [false, true] {
        let mut cmd = common::jevify_classifier(&server);
        cmd.args(["label", "bug,feature,question", "-0"]);
        if machine {
            cmd.arg("--json");
        }
        let out = cmd.write_stdin(input.as_slice()).output().unwrap();
        if machine {
            let value = envelope(&out, 0);
            let entries = value["data"]["records"].as_array().unwrap();
            assert_eq!(entries[2]["label"], "question");
            assert_eq!(entries[2]["lossy"], true);
            assert_eq!(entries[2]["ordinal"], 3);
            assert_eq!(entries[2]["text"], "\u{fffd} question\0");
            assert_eq!(entries[3]["label"], "?");
        } else {
            assert_eq!(out.status.code(), Some(0));
            let (labels, rest) = cut_labels(&out.stdout, 0);
            assert_eq!(labels, ["bug", "feature", "question", "?"]);
            assert_eq!(rest, b"bug\tone\0feature\0\xff question\0tie\0");
        }
    }
}

#[tokio::test]
async fn unsure_records_ties_and_none_are_question_marks_whatever_the_threshold() {
    let server = common::mock_classifier(fake()).await;
    for (input, threshold, expected, code) in [
        (
            "bug\nunsure\nclose\nnothing\ntie\n",
            None,
            "bug\t?\t?\t?\t?\t",
            0,
        ),
        (
            "bug\nunsure\nclose\nnothing\ntie\n",
            Some("0.99"),
            "bug\t?\t?\t?\t?\t",
            0,
        ),
        ("bug\nfeature\n", Some("0.99"), "bug\tfeature\t", 0),
        ("unsure\nnothing\n", None, "?\t?\t", 3),
        ("unsure\nnothing\n", Some("0.01"), "?\t?\t", 3),
    ] {
        for machine in [false, true] {
            let mut cmd = common::jevify_classifier(&server);
            cmd.args(["label", "bug,feature"]);
            if let Some(threshold) = threshold {
                cmd.env("JEVIFY_THRESHOLD", threshold);
            }
            if machine {
                cmd.arg("--json");
            }
            let out = cmd.write_stdin(input).output().unwrap();
            let unsure = expected.matches("?\t").count();
            if machine {
                let value = envelope(&out, code);
                assert_eq!(value["data"]["unsure"], unsure);
                let labels: String = value["data"]["records"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| format!("{}\t", e["label"].as_str().unwrap()))
                    .collect();
                assert_eq!(labels, expected);
            } else {
                assert_eq!(out.status.code(), Some(code));
                let (labels, rest) = cut_labels(&out.stdout, b'\n');
                assert_eq!(rest, input.as_bytes());
                assert_eq!(
                    labels.iter().map(|l| format!("{l}\t")).collect::<String>(),
                    expected
                );
                assert!(String::from_utf8_lossy(&out.stderr).contains(&format!(
                    "jevify label: labelled {} of {}, {unsure} unsure",
                    labels.len() - unsure,
                    labels.len()
                )));
            }
        }
    }
}

#[tokio::test]
async fn label_count_is_checked_against_the_window_before_any_request() {
    for classifier in [true, false] {
        let server = if classifier {
            common::mock_classifier(fake()).await
        } else {
            common::mock(fake()).await
        };
        let window = if classifier { 99 } else { 200 };
        for count in [window, window + 1] {
            let labels: Vec<String> = (0..count).map(|i| format!("label{i}")).collect();
            let mut cmd = if classifier {
                common::jevify_classifier(&server)
            } else {
                common::jevify(&server)
            };
            let out = cmd
                .args(["label", &labels.join(","), "--json"])
                .write_stdin("label7 here\nlabel0 there\n")
                .output()
                .unwrap();
            let value = envelope(&out, if count > window { 2 } else { 0 });
            if count > window {
                assert_eq!(value["error"]["kind"], "usage");
                let message = value["error"]["message"].as_str().unwrap();
                assert!(message.contains(&format!("{count} labels")), "{message}");
                assert!(message.contains(&format!("at most {window}")), "{message}");
                assert_eq!(value["meta"]["requests"], 0);
            } else {
                let entries = value["data"]["records"].as_array().unwrap();
                assert_eq!(entries[0]["label"], "label7");
                assert_eq!(entries[1]["label"], "label0");
            }
        }
        let posts = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method == "POST")
            .count();
        assert_eq!(posts, 1);
    }
}

fn input(count: usize) -> String {
    (0..count).map(|i| format!("record {i}\n")).collect()
}

#[tokio::test]
async fn batch_boundaries_ceiling_and_duplicates_judged_once() {
    for (count, expected) in [(2500, 42), (20_001, 0)] {
        let server = common::mock_classifier(fake()).await;
        let out = common::jevify_classifier(&server)
            .args(["label", "bug,feature", "--json"])
            .write_stdin(input(count))
            .output()
            .unwrap();
        let value = envelope(&out, if count > 20_000 { 6 } else { 0 });
        if count > 20_000 {
            assert_eq!(value["error"]["kind"], "too_many");
        } else {
            assert_eq!(value["data"]["labelled"], count);
            assert!(String::from_utf8_lossy(&out.stderr).contains(&format!(
                "{count} records, {count} distinct, {expected} requests"
            )));
        }
        let posts = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method == "POST")
            .count();
        assert_eq!(posts, expected);
    }
    let server = common::mock_classifier(fake()).await;
    let out = common::jevify_classifier(&server)
        .args(["label", "bug,feature"])
        .write_stdin("bug\nfeature\nbug\nbug\n")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        out.stdout,
        b"bug\tbug\nfeature\tfeature\nbug\tbug\nbug\tbug\n"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("4 records, 2 distinct, 1 requests"));
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(
        &requests
            .iter()
            .find(|r| r.url.path() == "/v1/classify")
            .unwrap()
            .body,
    )
    .unwrap();
    assert_eq!(body["items"], serde_json::json!(["bug", "feature"]));
}

#[tokio::test]
async fn empty_stdin_model_provenance_and_no_saved_input() {
    let server = common::mock_classifier(fake()).await;
    let out = common::jevify_classifier(&server)
        .args(["label", "bug,feature", "--json"])
        .write_stdin("")
        .output()
        .unwrap();
    assert_eq!(envelope(&out, 6)["error"]["kind"], "empty_input");

    let root = tempfile::tempdir().unwrap().keep();
    let server = common::mock_classifier(
        fake().with_model(|_| "ibm-granite/granite-4.0-h-micro, jev-fake".into()),
    )
    .await;
    let out = common::jevify_classifier(&server)
        .env("JEVIFY_CACHE_DIR", &root)
        .args(["label", "bug,feature", "--json"])
        .write_stdin("bug\n")
        .output()
        .unwrap();
    let value = envelope(&out, 0);
    assert_eq!(
        value["meta"]["model"],
        "ibm-granite/granite-4.0-h-micro, jev-fake"
    );
    assert!(value["data"].get("saved_input").is_none());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("answered by ibm-granite/granite-4.0-h-micro, not Jev")
    );
    assert!(!root.join("outputs").exists());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
}

#[tokio::test]
async fn file_excerpts_are_judged_and_secrets_are_withheld() {
    let server = common::mock_classifier(fake()).await;
    let root = tempfile::tempdir().unwrap().keep();
    std::fs::write(root.join("source.rs"), "VISIBLE_EXCERPT feature").unwrap();
    std::fs::write(root.join(".npmrc"), "PRIVATE_EXCERPT bug").unwrap();
    for machine in [false, true] {
        let mut cmd = common::jevify_classifier(&server);
        cmd.current_dir(&root)
            .args(["label", "bug,feature", "-0", "--files"]);
        if machine {
            cmd.arg("--json");
        }
        let out = cmd.write_stdin(b"./source.rs\0.npmrc\0").output().unwrap();
        if machine {
            let value = envelope(&out, 0);
            assert_eq!(value["data"]["records"][0]["text"], "./source.rs\0");
            assert_eq!(value["data"]["records"][0]["label"], "feature");
            assert_eq!(value["data"]["excerpts_withheld"], 1);
            assert_eq!(value["data"]["labelled"], 2);
        } else {
            assert_eq!(out.status.code(), Some(0));
            assert_eq!(out.stdout, b"feature\t./source.rs\0bug\t.npmrc\0");
            assert!(
                String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .any(|line| line == "jevify label: excerpts withheld: 1")
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

#[derive(Clone)]
struct Batches {
    requests: Arc<AtomicUsize>,
    quota: Option<&'static str>,
    delay_last: bool,
}

impl Respond for Batches {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let first = body["items"][0].as_str().unwrap();
        if first == "record 1020" {
            if let Some(code) = self.quota {
                return FakeJev::quota(code, 1);
            }
        }
        let response = common::FakeClassifier(fake()).respond(request);
        if self.delay_last && first != "record 0" {
            response.set_delay(Duration::from_secs(3))
        } else {
            response
        }
    }
}

async fn batches(quota: Option<&'static str>, delay_last: bool) -> (MockServer, Batches) {
    let server = MockServer::start().await;
    let responder = Batches {
        requests: Arc::new(AtomicUsize::new(0)),
        quota,
        delay_last,
    };
    Mock::given(method("POST"))
        .and(path("/v1/classify"))
        .respond_with(responder.clone())
        .mount(&server)
        .await;
    (server, responder)
}

fn labelled(count: usize) -> String {
    (0..count).map(|i| format!("bug\trecord {i}\n")).collect()
}

#[tokio::test]
async fn daily_quota_keeps_the_answered_prefix() {
    for machine in [false, true] {
        let (server, responder) = batches(Some("rate_limit_day"), false).await;
        let mut cmd = common::jevify_classifier(&server);
        cmd.env("JEVIFY_CONCURRENCY", "1")
            .args(["label", "bug,feature"]);
        if machine {
            cmd.arg("--json");
        }
        let out = cmd.write_stdin(input(2500)).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        if machine {
            let value = envelope(&out, 4);
            assert_eq!(value["error"]["kind"], "api_unavailable");
            let message = value["error"]["message"].as_str().unwrap();
            assert!(message.contains("answered 1020 of 2500"), "{message}");
            assert!(message.contains("daily quota of the free backend reached"));
        } else {
            assert_eq!(out.status.code(), Some(4), "{stderr}");
            assert_eq!(out.stdout, labelled(1020).as_bytes());
            let status = stderr.find("jevify label: answered 1020 of 2500").unwrap();
            let error = stderr
                .find("daily quota of the free backend reached")
                .unwrap();
            assert!(status < error);
        }
        let requests = responder.requests.load(Ordering::SeqCst);
        // The 18th request (records 1020..1079) meets the quota; a 19th may be in flight.
        assert!((18..=19).contains(&requests), "{requests}");
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
        .env("JEVIFY_CONFIG_DIR", tempfile::tempdir().unwrap().keep())
        .env("JEVIFY_NO_CACHE", "1")
        .env("JEVIFY_CONCURRENCY", "1")
        .args(["label", "bug,feature"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// Two keyless batches of 60 records, the second delayed 3 s: the first line reaches stdout
/// within 2 s while the child still runs, so answers flow before the last one arrives.
#[tokio::test]
async fn stdout_flows_before_last_answer_and_closed_pipe_cancels() {
    let (server, _) = batches(None, true).await;
    let mut child = process(&server).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(input(120).as_bytes()).unwrap());
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
        "bug\trecord 0\n"
    );
    assert!(child.try_wait().unwrap().is_none());
    assert!(child.wait().unwrap().success());
    writer.join().unwrap();
    assert_eq!(reader.join().unwrap(), labelled(120));

    let (server, responder) = batches(None, true).await;
    let mut child = process(&server).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(input(10_074).as_bytes()).unwrap());
    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    for i in 0..3 {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, format!("bug\trecord {i}\n"));
    }
    drop(reader);
    assert_eq!(child.wait().unwrap().code(), Some(0));
    writer.join().unwrap();
    assert!(responder.requests.load(Ordering::SeqCst) < 11);
}

/// A file jevify cannot read is not judged on its name: it comes out `?` with p 0, is counted
/// with the withheld excerpts, is named on stderr with the reason, and is never sent.
#[tokio::test]
async fn unreadable_files_are_named_counted_and_left_unsure_not_labelled_by_name() {
    use std::os::unix::fs::PermissionsExt;
    let server = common::mock_classifier(fake()).await;
    let root = tempfile::tempdir().unwrap().keep();
    std::fs::write(root.join("readable.md"), "VISIBLE_EXCERPT feature").unwrap();
    std::fs::create_dir(root.join("adir")).unwrap();
    let denied = root.join("denied.md");
    std::fs::write(&denied, "VISIBLE_EXCERPT feature").unwrap();
    std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Under root the permission bits do not deny; the directory and the missing file do.
    let denied_unreadable = std::fs::read(&denied).is_err();
    let unreadable = 2 + usize::from(denied_unreadable);
    let denied_label = if denied_unreadable { "?" } else { "feature" };
    for machine in [false, true] {
        let mut cmd = common::jevify_classifier(&server);
        cmd.current_dir(&root)
            .args(["label", "bug,feature", "-0", "--files"]);
        if machine {
            cmd.arg("--json");
        }
        let out = cmd
            .write_stdin(b"./readable.md\0adir\0denied.md\0missing.md\0")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr
                .lines()
                .any(|line| line == "jevify label: excerpt unreadable: adir: is a directory"),
            "{stderr}"
        );
        assert!(
            stderr.contains("jevify label: excerpt unreadable: missing.md: No such file"),
            "{stderr}"
        );
        assert_eq!(
            stderr.contains("excerpt unreadable: denied.md: Permission denied"),
            denied_unreadable,
            "{stderr}"
        );
        assert!(
            stderr.contains(&format!(
                "labelled {} of 4, {} unsure, excerpts withheld: {unreadable}",
                4 - unreadable,
                unreadable
            )),
            "{stderr}"
        );
        if machine {
            let value = envelope(&out, 0);
            let records = value["data"]["records"].as_array().unwrap();
            assert_eq!(records[0]["label"], "feature");
            assert!(records[0].get("unreadable").is_none());
            assert_eq!(records[1]["label"], "?");
            assert_eq!(records[1]["p"], 0.0);
            assert_eq!(records[1]["unreadable"], "is a directory");
            assert_eq!(records[2]["label"], denied_label);
            assert_eq!(records[3]["label"], "?");
            assert!(
                records[3]["unreadable"]
                    .as_str()
                    .unwrap()
                    .contains("No such file")
            );
            assert_eq!(value["data"]["excerpts_withheld"], unreadable);
            assert_eq!(value["data"]["unsure"], unreadable);
            assert_eq!(value["data"]["labelled"], 4 - unreadable);
        } else {
            assert_eq!(out.status.code(), Some(0), "{stderr}");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                format!(
                    "feature\t./readable.md\0?\tadir\0{denied_label}\tdenied.md\0?\tmissing.md\0"
                )
            );
            assert!(
                stderr
                    .lines()
                    .any(|line| line == format!("jevify label: excerpts withheld: {unreadable}")),
                "{stderr}"
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
        assert!(body.contains("VISIBLE_EXCERPT"), "{body}");
        assert!(!body.contains("adir"), "{body}");
        assert!(!body.contains("missing.md"), "{body}");
        assert_eq!(body.contains("denied.md"), !denied_unreadable, "{body}");
    }

    // Nothing readable: every record is unsure, exit 3, and no request is made.
    let mut cmd = common::jevify_classifier(&server);
    let before = server.received_requests().await.unwrap().len();
    let out = cmd
        .current_dir(&root)
        .args(["label", "bug,feature", "-0", "--files"])
        .write_stdin(b"adir\0missing.md\0")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{stderr}");
    assert_eq!(out.stdout, b"?\tadir\0?\tmissing.md\0");
    assert!(
        stderr.contains("labelled 0 of 2, 2 unsure, excerpts withheld: 2"),
        "{stderr}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), before);
}
