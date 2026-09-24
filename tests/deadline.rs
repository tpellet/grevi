//! The overall budget (`JEVIFY_DEADLINE`) covers the whole verb, the evidence read included:
//! a record source that blocks past the budget ends the verb at exit 4, naming the deadline,
//! and no request is sent.

mod common;

use assert_cmd::cargo::CommandCargoExt;
use common::FakeJev;
use serde_json::Value;
use std::io::Write;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn fake() -> FakeJev {
    FakeJev {
        choose: |_, _, options| {
            options
                .iter()
                .find(|option| option.ends_with("statement holds"))
                .unwrap_or(&options[0])
                .clone()
        },
        noul: |_, _| 0.9,
    }
}

/// Runs the verb with a one-second budget while stdin stays open for `hold`: the evidence
/// read blocks that long before the first request could be built.
async fn blocked_read(verb: &[&str], hold: Duration) -> (std::process::Output, usize) {
    blocked_read_in(verb, hold, &["--json"]).await
}

/// The same run in a chosen output format, for the sentence a person reads on stderr.
async fn blocked_read_in(
    verb: &[&str],
    hold: Duration,
    format: &[&str],
) -> (std::process::Output, usize) {
    let server = common::mock_classifier(fake()).await;
    let mut cmd = std::process::Command::cargo_bin("jevify").unwrap();
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
        .env("JEVIFY_DEADLINE", "1")
        .args(verb)
        .args(format)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"Cargo.toml\n").unwrap();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(hold);
        drop(stdin);
    });
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap();
    let posts = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method == "POST")
        .count();
    (out, posts)
}

fn envelope(out: &std::process::Output) -> Value {
    let parsed = serde_json::from_slice(&out.stdout);
    assert!(
        parsed.is_ok(),
        "{:?}: {}\n{}",
        parsed.as_ref().err(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    parsed.unwrap()
}

/// `filter --files` reads the records and their excerpts before it builds the client, and
/// `pick` builds the client first: the budget counts from the verb's start in both.
#[tokio::test]
async fn a_blocked_evidence_read_past_the_budget_ends_at_exit_4_without_a_request() {
    for verb in [
        &["filter", "--files", "--no-save", "x"][..],
        &["pick", "the manifest"][..],
    ] {
        let start = Instant::now();
        let (out, posts) = blocked_read(verb, Duration::from_millis(1500)).await;
        let value = envelope(&out);
        assert_eq!(out.status.code(), Some(4), "{verb:?}: {value}");
        assert_eq!(value["exit_code"], 4, "{verb:?}: {value}");
        let message = value["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("deadline of 1 s") && message.contains("JEVIFY_DEADLINE"),
            "{verb:?}: {value}"
        );
        // Exit 4 carries three situations a caller answers differently. Deadline expiry says
        // raise the budget or shard; it is not the transport failure `api_unavailable`, and
        // the caller reads which from the kind, never from the message.
        assert_eq!(value["error"]["kind"], "api_deadline", "{verb:?}: {value}");
        assert!(
            message.contains(jevify::exit::DEADLINE_PREFIX),
            "the deadline message and exit.rs's prefix drifted apart: {message}"
        );
        // The deadline has its own sentence. It is jevify's own budget that ran out, not the
        // API that went missing, and the sentence names the variable that sets the budget.
        assert!(
            !message.contains("API unavailable"),
            "{verb:?}: the deadline still reads as an outage: {message}"
        );
        let hint = value["error"]["hint"].as_str().unwrap_or_default();
        assert!(hint.contains("JEVIFY_DEADLINE"), "{verb:?}: {value}");
        assert_eq!(posts, 0, "{verb:?}: a request was sent after the deadline");
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{verb:?}: {:?}",
            start.elapsed()
        );
    }
}

/// A read that ends inside the budget is judged as usual.
#[tokio::test]
async fn a_read_inside_the_budget_is_judged() {
    let (out, posts) = blocked_read(
        &["filter", "--files", "--no-save", "x"],
        Duration::from_millis(100),
    )
    .await;
    let value = envelope(&out);
    assert_eq!(out.status.code(), Some(0), "{value}");
    assert_eq!(posts, 1, "{value}");
}

/// The sentence a person reads on stderr is the deadline's own, not the shared unavailable one.
#[tokio::test]
async fn the_human_sentence_names_the_deadline_and_the_variable() {
    let (out, posts) = blocked_read_in(
        &["filter", "--files", "--no-save", "x"],
        Duration::from_millis(1500),
        &[],
    )
    .await;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(4), "{stderr}");
    assert_eq!(posts, 0, "{stderr}");
    assert!(
        stderr.contains("the overall deadline of 1 s passed before the answer was ready; JEVIFY_DEADLINE sets it"),
        "{stderr}"
    );
    assert!(!stderr.contains("API unavailable"), "{stderr}");
    assert!(stderr.contains("raise JEVIFY_DEADLINE"), "{stderr}");
}
