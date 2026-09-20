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
        c.args([
            "--json",
            "run",
            "--dry-run",
            "--no-args",
            "burn",
            "a",
            "dvd",
        ])
        .output()
        .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["tool"], "drutil", "{v}");
    assert_eq!(out.status.code(), Some(0));
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
        c.args(["run", "--dry-run", "make", "a", "qr", "code"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("nothing installed")
    );
}
