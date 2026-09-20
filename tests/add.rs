mod common;
use common::FakeJev;
use std::process::Command as P;

fn git(dir: &std::path::Path, args: &[&str]) {
    assert!(
        P::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn stages_only_matching_hunks() {
    let server = common::mock(FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |i, s| {
            let idx: usize = i
                .split("hunks[")
                .nth(1)
                .and_then(|r| r.split(']').next())
                .and_then(|n| n.parse().ok())
                .unwrap();
            let h = s["hunks"][idx].as_str().unwrap_or_default().to_string();
            if h.contains("AUTH") { 0.95 } else { 0.05 }
        },
    })
    .await;
    let d = tempfile::tempdir().unwrap();
    git(d.path(), &["init", "-q"]);
    git(d.path(), &["config", "user.email", "t@t"]);
    git(d.path(), &["config", "user.name", "t"]);
    git(d.path(), &["config", "commit.gpgsign", "false"]);
    let body: String = (0..40).map(|i| format!("line {i}\n")).collect();
    std::fs::write(d.path().join("f.txt"), &body).unwrap();
    git(d.path(), &["add", "."]);
    git(d.path(), &["commit", "-qm", "init"]);
    let changed = body
        .replace("line 2\n", "line 2 AUTH fix\n")
        .replace("line 35\n", "line 35 typo\n");
    std::fs::write(d.path().join("f.txt"), changed).unwrap();
    let mut c = common::grevi(&server);
    c.current_dir(d.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["add", "--yes", "the auth fix"]).output().unwrap()
    })
    .await
    .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let staged = String::from_utf8(
        P::new("git")
            .args(["diff", "--cached"])
            .current_dir(d.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert!(staged.contains("AUTH") && !staged.contains("typo"));
}

#[tokio::test(flavor = "multi_thread")]
async fn machine_mode_without_yes_is_declined() {
    let server = common::mock(FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |_, _| 0.95,
    })
    .await;
    let d = tempfile::tempdir().unwrap();
    git(d.path(), &["init", "-q"]);
    git(d.path(), &["config", "user.email", "t@t"]);
    git(d.path(), &["config", "user.name", "t"]);
    git(d.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(d.path().join("f.txt"), "a\n").unwrap();
    git(d.path(), &["add", "."]);
    git(d.path(), &["commit", "-qm", "init"]);
    std::fs::write(d.path().join("f.txt"), "b\n").unwrap();
    let mut c = common::grevi(&server);
    c.current_dir(d.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "add", "anything"]).output().unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(130));
}

// From a subdirectory, hunks in files outside it must still be staged (git runs at the top level;
// `git apply` from `sub/` would skip `top.txt` and still exit 0).
#[tokio::test(flavor = "multi_thread")]
async fn stages_from_a_subdirectory() {
    let server = common::mock(FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |_, _| 0.95,
    })
    .await;
    let d = tempfile::tempdir().unwrap();
    git(d.path(), &["init", "-q"]);
    git(d.path(), &["config", "user.email", "t@t"]);
    git(d.path(), &["config", "user.name", "t"]);
    git(d.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::create_dir(d.path().join("sub")).unwrap();
    std::fs::write(d.path().join("top.txt"), "a\n").unwrap();
    git(d.path(), &["add", "."]);
    git(d.path(), &["commit", "-qm", "init"]);
    std::fs::write(d.path().join("top.txt"), "b\n").unwrap();
    let mut c = common::grevi(&server);
    c.current_dir(d.path().join("sub"));
    let out =
        tokio::task::spawn_blocking(move || c.args(["add", "--yes", "anything"]).output().unwrap())
            .await
            .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let staged = String::from_utf8(
        P::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(d.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(staged.trim(), "top.txt");
}

// A clean tree has nothing for `add` to stage: it must exit 6 (input) with a hint specific to
// `add`, not the generic stdin hint ("pipe text into grevi") that fits `pick`/`why`/`sort` but
// not `add` (which reads `git diff`, not stdin).
#[tokio::test(flavor = "multi_thread")]
async fn clean_tree_exits_with_an_add_specific_hint() {
    let server = common::mock(FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |_, _| 0.05,
    })
    .await;
    let d = tempfile::tempdir().unwrap();
    git(d.path(), &["init", "-q"]);
    git(d.path(), &["config", "user.email", "t@t"]);
    git(d.path(), &["config", "user.name", "t"]);
    git(d.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(d.path().join("f.txt"), "a\n").unwrap();
    git(d.path(), &["add", "."]);
    git(d.path(), &["commit", "-qm", "init"]);
    let mut c = common::grevi(&server);
    c.current_dir(d.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "add", "anything"]).output().unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(6));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["kind"], "empty_input");
    let hint = v["error"]["hint"].as_str().unwrap();
    assert!(hint.contains("git diff"), "{hint}");
    assert!(!hint.contains("pipe text into grevi"), "{hint}");
    assert_eq!(v["error"]["example"], "grevi add \"finish the login flow\"");
}
