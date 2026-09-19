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
    let mut c = common::hunch(&server);
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

// `sh` here is the user's chosen command run via argv; hunch itself never uses a shell.
#[tokio::test(flavor = "multi_thread")]
async fn why_runs_a_command_after_double_dash() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "E0432"),
        noul: |_, _| 0.93,
    })
    .await;
    let mut c = common::hunch(&server);
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
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert!(
        v["data"]["causes"][0]["text"]
            .as_str()
            .unwrap()
            .contains("E0432")
    );
    assert_eq!(v["data"]["child_exit"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn no_signal_on_stdin_hints_at_stderr() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.05,
    })
    .await;
    let mut c = common::hunch(&server);
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
