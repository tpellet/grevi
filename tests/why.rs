mod common;
use common::{FakeJev, option_containing};

#[tokio::test(flavor = "multi_thread")]
async fn points_at_root_cause_with_context() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "E0432"),
        noul: |_, _| 0.93,
    })
    .await;
    let log = "   Compiling foo v0.1.0\nerror[E0432]: unresolved import `bar`\n --> src/main.rs:1:5\nerror: could not compile `foo`\n";
    let mut c = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "why", "-C", "1"])
            .write_stdin(log)
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(v["data"]["causes"][0]["line"], 2);
    assert_eq!(
        v["data"]["causes"][0]["context"].as_array().unwrap().len(),
        3
    );
}

// The removed child-command form is rejected before requests or execution.
#[tokio::test(flavor = "multi_thread")]
async fn why_rejects_a_command_after_double_dash() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "E0432"),
        noul: |_, _| 0.93,
    })
    .await;
    let mut c = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args([
            "--json",
            "why",
            "--",
            "sh",
            "-c",
            "echo ok; echo 'error[E0432]: boom' >&2; exit 1",
        ])
        .output()
        .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(2), "{v}");
    assert_eq!(v["error"]["kind"], "usage");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("CMD 2>&1 | jevify why")
    );
    assert_eq!(v["meta"]["requests"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn no_save_keeps_the_same_requests_and_numbered_output() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "error"),
        noul: |_, _| 0.93,
    })
    .await;
    let mut outputs = Vec::new();
    for no_save in [false, true] {
        let mut cmd = common::jevify(&server);
        cmd.arg("why");
        if no_save {
            cmd.arg("--no-save");
        }
        outputs.push(
            tokio::task::spawn_blocking(move || {
                cmd.write_stdin("before\nerror: boom\nafter\n")
                    .output()
                    .unwrap()
            })
            .await
            .unwrap(),
        );
    }
    assert!(outputs.iter().all(|out| out.status.success()));
    assert_eq!(outputs[0].stdout, outputs[1].stdout);
    assert!(String::from_utf8_lossy(&outputs[0].stdout).contains(">     2 │ error: boom"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len() % 2, 0);
    let (first_run, second_run) = requests.split_at(requests.len() / 2);
    for (first, second) in first_run.iter().zip(second_run) {
        assert_eq!(first.body, second.body);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_signal_on_stdin_hints_at_stderr() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.05,
    })
    .await;
    let mut c = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "why"])
            .write_stdin("   Compiling foo v0.1.0\n   Compiling bar v0.2.0\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(v["data"]["hint"].as_str().unwrap().contains("2>&1"));
}
