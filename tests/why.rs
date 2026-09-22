mod common;
use common::{FakeJev, option_containing};

#[tokio::test(flavor = "multi_thread")]
async fn classifier_max_keep_and_single_window_preserve_context_finals() {
    for count in [3, jevify::cmd::why::MAX_KEEP] {
        let server = common::mock_classifier(FakeJev {
            choose: |_, s, o| option_containing(s, o, "ROOT"),
            noul: |_, _| 0.95,
        })
        .await;
        let mut lines: Vec<_> = (0..count).map(|i| format!("error step {i}")).collect();
        lines[count - 1] = "error ROOT".into();
        lines[0] = "test step 0 ... FAILED".into();
        let input = format!("{}\n", lines.join("\n"));
        let mut cmd = common::jevify_classifier(&server);
        let out = tokio::task::spawn_blocking(move || {
            cmd.args(["--json", "why", "--no-save"])
                .write_stdin(input)
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(out.status.code(), Some(0));
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["data"]["considered"], count);
        assert_eq!(value["data"]["causes"][0]["line"], count);
        assert_eq!(value["data"]["causes"][0]["text"], "error ROOT");
        assert!(value["data"]["causes"][0]["context"].is_array());
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), if count == 3 { 2 } else { 42 });
        let body: serde_json::Value =
            serde_json::from_slice(&requests.last().unwrap().body).unwrap();
        let state: serde_json::Value =
            serde_json::from_str(body["items"][0].as_str().unwrap()).unwrap();
        let finalists = state["items"].as_array().unwrap();
        assert_eq!(finalists.len(), if count == 3 { 3 } else { 82 });
        assert!(finalists.iter().all(|s| {
            s.as_str()
                .unwrap()
                .contains("context only, not a candidate; before:")
        }));
        // The finals carry the nearest failure statement (`FAILED` on line 1) as context.
        assert!(finalists.iter().any(|s| {
            s.as_str()
                .unwrap()
                .contains("not a candidate; nearest failure statement, ")
        }));
        if count > 3 {
            assert!(String::from_utf8_lossy(&out.stderr).contains("finalists per window: 2"));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn saves_exact_bytes_once_and_reports_path_even_with_verbose() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "error"),
        noul: |_, _| 0.93,
    })
    .await;
    let root = tempfile::tempdir().unwrap().keep();
    let input = b"before\r\nerror: bad \xff\r\nafter\r\n";
    let mut paths = Vec::new();
    for _ in 0..2 {
        let mut cmd = common::jevify(&server);
        cmd.env("JEVIFY_CACHE_DIR", &root);
        let out = tokio::task::spawn_blocking(move || {
            cmd.args(["--json", "--verbose", "why"])
                .write_stdin(input.as_slice())
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(out.status.code(), Some(0));
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["data"]["complete"], true);
        let path = value["data"]["saved_input"].as_str().unwrap();
        assert!(std::path::Path::new(path).starts_with(&root));
        assert_eq!(std::fs::read(path).unwrap(), input);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            stderr
                .lines()
                .filter(|l| *l == format!("jevify why: full output: {path}"))
                .count(),
            1
        );
        paths.push(path.to_owned());
    }
    assert_eq!(paths[0], paths[1]);
    assert_eq!(std::fs::read_dir(root.join("outputs")).unwrap().count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn skipped_and_failed_saves_are_incomplete_and_fill_not_run_is_searchable() {
    use std::os::unix::fs::PermissionsExt;
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "not run"),
        noul: |_, _| 0.93,
    })
    .await;
    for no_save in [true, false] {
        let root = tempfile::tempdir().unwrap().keep();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o500)).unwrap();
        let mut cmd = common::jevify(&server);
        cmd.env("JEVIFY_CACHE_DIR", &root).args(["--json", "why"]);
        if no_save {
            cmd.arg("--no-save");
        }
        let out = tokio::task::spawn_blocking(move || {
            cmd.write_stdin("jevify fill: not run: ambiguous\n")
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(out.status.code(), Some(0));
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["data"]["complete"], false);
        assert!(value["data"]["saved_input"].is_null());
        assert_eq!(
            value["data"]["causes"][0]["text"],
            "jevify fill: not run: ambiguous"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("jevify why: full output: not saved (")
        );
        assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
    }
}

#[test]
fn why_rejects_nul_split_without_saving() {
    common::bin()
        .args(["why", "-0", "--no-save"])
        .write_stdin("error\0")
        .assert()
        .code(2);
}

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
    // The count line precedes the first request, like filter and label.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("jevify why: 4 lines, candidates 4, windows 1"),
        "{stderr}"
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
