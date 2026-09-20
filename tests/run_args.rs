mod common;
use common::FakeJev;

fn inv(dir: &tempfile::TempDir, json: &str) -> std::path::PathBuf {
    let p = dir.path().join("inv.json");
    std::fs::write(&p, json).unwrap();
    p
}

#[tokio::test(flavor = "multi_thread")]
async fn machine_mode_never_executes_without_exec_and_yes() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "true"),
        noul: |_, _| 0.95,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::grevi(&server);
    c.env(
        "GREVI_INVENTORY_FILE",
        inv(
            &dir,
            r#"[{"name":"true","summary":"do nothing, successfully"}]"#,
        ),
    )
    .current_dir(dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "run", "--yes", "succeed", "quietly"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["executed"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn exec_and_yes_run_the_command_via_argv() {
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
    let mut c = common::grevi(&server);
    c.env(
        "GREVI_INVENTORY_FILE",
        inv(
            &dir,
            r#"[{"name":"true","summary":"do nothing, successfully"}]"#,
        ),
    )
    .current_dir(dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "run", "--exec", "--yes", "succeed", "quietly"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["executed"], true);
    assert_eq!(v["data"]["child_exit"], 0);
}

// `ls` prints to stdout; in machine mode that output must land on stderr, not in the envelope.
#[tokio::test(flavor = "multi_thread")]
async fn executed_child_output_never_corrupts_the_envelope() {
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
    let mut c = common::grevi(&server);
    c.env(
        "GREVI_INVENTORY_FILE",
        inv(
            &dir,
            r#"[{"name":"ls","summary":"list directory contents"}]"#,
        ),
    )
    .current_dir(dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "run", "--exec", "--yes", "list", "files"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["executed"], true);
    assert!(String::from_utf8(out.stderr).unwrap().contains("inv.json"));
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
    let mut c = common::grevi(&server);
    c.env(
        "GREVI_INVENTORY_FILE",
        inv(
            &dir,
            r#"[{"name":"rm","summary":"remove directory entries"}]"#,
        ),
    )
    .current_dir(dir.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args([
            "--json", "run", "--exec", "--yes", "delete", "the", "temp", "file",
        ])
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
        .stdout(predicates::str::contains("alias ,="));
}
