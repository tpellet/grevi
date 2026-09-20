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
        let mut c = common::grevi(&server);
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
        let mut c = common::grevi(&server);
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
    let mut c = common::grevi(&server);
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
