mod common;
use common::FakeJev;

#[tokio::test(flavor = "multi_thread")]
async fn unsupported_cp_grammar_never_executes_even_without_placeholders() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "cp"),
        noul: |_, _| 0.95,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::jevify(&server);
    c.env(
        "JEVIFY_INVENTORY_FILE",
        inv(&dir, r#"[{"name":"cp","summary":"copy files"}]"#),
    )
    .current_dir(dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "route", "copy source.txt to destination.txt"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(v["data"]["complete"], false);
    assert_eq!(v["data"]["executed"], false);
    assert!(
        v["data"]["blocked"]
            .as_str()
            .unwrap()
            .contains("unvalidated command grammar")
    );
}

fn inv(dir: &tempfile::TempDir, json: &str) -> std::path::PathBuf {
    let p = dir.path().join("inv.json");
    std::fs::write(&p, json).unwrap();
    p
}

#[tokio::test(flavor = "multi_thread")]
async fn removed_execution_flags_are_usage_errors() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "true"),
        noul: |_, _| 0.95,
    })
    .await;
    for flag in ["--yes", "--exec", "--dry-run", "--no-args"] {
        let mut cmd = common::jevify(&server);
        tokio::task::spawn_blocking(move || {
            let out = cmd
                .args(["--json", "route", "succeed", flag])
                .output()
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(v["error"]["kind"], "usage");
            assert_eq!(v["meta"]["requests"], 0);
        })
        .await
        .unwrap();
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn route_prints_without_starting_the_selected_tool() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "true"),
        noul: |i, _| {
            if i.contains("ask for the behaviour") {
                0.0
            } else {
                0.95
            }
        },
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    let tool = dir.path().join("true");
    std::fs::write(&tool, "#!/bin/sh\nprintf started > sentinel\n").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut c = common::jevify(&server);
    c.env(
        "JEVIFY_INVENTORY_FILE",
        inv(
            &dir,
            r#"[{"name":"true","summary":"do nothing, successfully"}]"#,
        ),
    )
    .current_dir(dir.path())
    .env("PATH", dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["route", "succeed", "quietly"]).output().unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"'true'\n");
    assert!(!dir.path().join("sentinel").exists());
}

// A routed `ls` starts no child and prints only the envelope.
#[tokio::test(flavor = "multi_thread")]
async fn route_machine_output_is_one_envelope_without_child_output() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "ls"),
        noul: |i, _| {
            if i.contains("ask for the behaviour") {
                0.0
            } else {
                0.95
            }
        },
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::jevify(&server);
    c.env(
        "JEVIFY_INVENTORY_FILE",
        inv(
            &dir,
            r#"[{"name":"ls","summary":"list directory contents"}]"#,
        ),
    )
    .current_dir(dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "route", "list", "files"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(v["data"]["executed"], false);
    assert_eq!(v["command"], "route");
    assert!(out.stderr.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn never_exec_tools_are_shown_not_run() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "rm"),
        noul: |i, _| {
            if i.contains("ask for the behaviour") {
                0.0
            } else {
                0.95
            }
        },
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::jevify(&server);
    c.env(
        "JEVIFY_INVENTORY_FILE",
        inv(
            &dir,
            r#"[{"name":"rm","summary":"remove directory entries"}]"#,
        ),
    )
    .current_dir(dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "route", "delete", "the", "temp", "file"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["data"]["executed"], false);
    assert_eq!(v["data"]["argv"][0], "rm");
    assert!(v["data"]["blocked"].as_str().unwrap().contains("rm"));
    assert!(dir.path().join("inv.json").exists());
}

#[test]
fn init_zsh_defines_comma_alias() {
    common::bin()
        .args(["init", "zsh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("alias ,='noglob jevify route'"));
}
