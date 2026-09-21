mod common;
use common::FakeJev;

#[tokio::test(flavor = "multi_thread")]
async fn route_does_not_propose_arguments() {
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
    assert_eq!(v["data"]["tool"], "cp");
    for field in [
        "argv",
        "flags",
        "executed",
        "child_exit",
        "blocked",
        "complete",
    ] {
        assert!(v["data"].get(field).is_none(), "{field}: {v}");
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.iter().filter(|r| r.method == "POST").count(), 2);
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
        choose: |_, s, o| common::option_containing(s, o, "macho-inspect"),
        noul: |_, _| 0.95,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    let tool = dir.path().join("macho-inspect");
    std::fs::write(&tool, "#!/bin/sh\nprintf started > sentinel\n").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut c = common::jevify(&server);
    c.env(
        "JEVIFY_INVENTORY_FILE",
        inv(
            &dir,
            r#"[{"name":"macho-inspect","summary":"inspect Mach-O metadata"}]"#,
        ),
    )
    .current_dir(dir.path())
    .env("PATH", dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["route", "inspect Mach-O metadata"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"macho-inspect\n");
    assert!(String::from_utf8_lossy(&out.stderr).contains("inspect Mach-O metadata"));
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
    .current_dir(dir.path())
    .env("PATH", dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "route", "list", "files"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(v["data"].get("executed").is_none());
    assert_eq!(v["data"]["synopsis"], serde_json::Value::Null);
    assert!(v["data"].get("synopsis").is_some());
    assert_eq!(v["command"], "route");
    assert!(out.stderr.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn destructive_tools_are_shown_without_an_execution_policy() {
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
    assert!(v["data"].get("executed").is_none());
    assert_eq!(v["data"]["tool"], "rm");
    assert!(v["data"].get("blocked").is_none());
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
