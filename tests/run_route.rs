mod common;
use common::FakeJev;

fn inv(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let p = dir.path().join("inv.json");
    std::fs::write(&p, r#"[{"name":"tar","summary":"manipulate tape archives"},{"name":"curl","summary":"transfer a URL"},{"name":"drutil","summary":"interact with CD/DVD burners"}]"#).unwrap();
    p
}

#[tokio::test(flavor = "multi_thread")]
async fn routes_to_the_tool() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "drutil"),
        noul: |i, s| {
            if i.contains("a correct, direct way") {
                if s.to_string().contains("drutil") && i.contains("[0]") {
                    0.9
                } else {
                    0.2
                }
            } else {
                0.9
            }
        },
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::jevify(&server);
    c.env("JEVIFY_INVENTORY_FILE", inv(&dir));
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "route", "burn", "a", "dvd"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["tool"], "drutil", "{v}");
    assert_eq!(v["command"], "route");
    assert_eq!(v["data"]["summary"], "interact with CD/DVD burners");
    assert!(v["data"]["fit"].is_number());
    assert!(v["data"]["alternatives"].is_array());
    assert_eq!(
        v["data"]["ties"],
        serde_json::json!([]),
        "a clear winner has no tie: {v}"
    );
    assert!(v["data"].get("synopsis").is_some());
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
    assert_eq!(out.status.code(), Some(0));
}

/// Two commands that fit within the tie margin: `route` names the best on stdout and the tied
/// one in `ties`, and abstains on neither. A command below the threshold is never a tie.
#[tokio::test(flavor = "multi_thread")]
async fn a_near_tie_names_both() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "tar"),
        noul: |i, s| {
            if !i.contains("a correct, direct way") {
                return 0.9;
            }
            let k: usize = i
                .split("commands[")
                .nth(1)
                .and_then(|r| r.split(']').next())
                .and_then(|n| n.parse().ok())
                .unwrap();
            let described = s["commands"][k].as_str().unwrap();
            if described.starts_with("tar:") {
                0.9
            } else if described.starts_with("curl:") {
                0.88
            } else {
                0.2
            }
        },
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::jevify(&server);
    c.env("JEVIFY_INVENTORY_FILE", inv(&dir));
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "route", "fetch", "an", "archive"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["data"]["tool"], "tar", "{v}");
    assert_eq!(v["data"]["ties"].as_array().map(Vec::len), Some(1), "{v}");
    assert_eq!(v["data"]["ties"][0]["tool"], "curl", "{v}");
    assert!(v["data"]["ties"][0]["fit"].is_number());
    assert_eq!(
        v["data"]["alternatives"].as_array().map(Vec::len),
        Some(2),
        "{v}"
    );
    let mut c = common::jevify(&server);
    c.env("JEVIFY_INVENTORY_FILE", inv(&dir));
    let out = tokio::task::spawn_blocking(move || {
        c.args(["route", "fetch", "an", "archive"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"tar\n");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("also fits: curl (0.88)"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn abstains_when_nothing_installed_fits() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.05,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::jevify(&server);
    c.env("JEVIFY_INVENTORY_FILE", inv(&dir));
    let out = tokio::task::spawn_blocking(move || {
        c.args(["route", "make", "a", "qr", "code"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("nothing installed")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_inventory_abstains_without_a_request() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.0,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let inventory = dir.path().join("empty.json");
    std::fs::write(&inventory, "[]").unwrap();
    for machine in [false, true] {
        let mut cmd = common::jevify(&server);
        cmd.env("JEVIFY_INVENTORY_FILE", &inventory);
        let out = tokio::task::spawn_blocking(move || {
            if machine {
                cmd.arg("--json");
            }
            cmd.args(["route", "inspect Mach-O metadata"])
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(out.status.code(), Some(3));
        if machine {
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(v["command"], "route");
            assert!(v["data"]["tool"].is_null());
            assert!(v["data"]["synopsis"].is_null());
            assert_eq!(v["data"]["alternatives"], serde_json::json!([]));
            assert_eq!(v["meta"]["requests"], 0);
        } else {
            assert!(out.stdout.is_empty());
        }
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method != "POST")
    );
}

#[test]
fn empty_intent_is_a_usage_error() {
    for args in [vec!["route"], vec!["route", ""], vec!["route", " \t "]] {
        common::bin().args(args).assert().code(2);
    }
}
