mod common;
use common::FakeJev;

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_then_apply_then_undo() {
    let server = common::mock(FakeJev {
        choose: |i, s, o| {
            // file k -> "Finance" folder if its text mentions invoice
            let k: usize = i
                .split("files[")
                .nth(1)
                .and_then(|r| r.split(']').next())
                .and_then(|n| n.parse().ok())
                .unwrap();
            let f = s["files"][k].as_str().unwrap_or_default();
            if f.contains("invoice") {
                common::option_containing(&serde_json::json!({"items": s["folders"]}), o, "Finance")
            } else {
                "NONE".into()
            }
        },
        // The absolute Noul gates the move: a file the Choice would file under Finance but whose
        // Noul says nothing fits ("unsure") must stay where it is.
        noul: |i, s| {
            let k: usize = i
                .split("files[")
                .nth(1)
                .and_then(|r| r.split(']').next())
                .and_then(|n| n.parse().ok())
                .unwrap();
            if s["files"][k]
                .as_str()
                .unwrap_or_default()
                .contains("unsure")
            {
                0.2
            } else {
                0.9
            }
        },
    })
    .await;
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("Finance")).unwrap();
    std::fs::write(d.path().join("march.txt"), "invoice total 42 EUR").unwrap();
    std::fs::write(d.path().join("poem.txt"), "roses are red").unwrap();
    std::fs::write(d.path().join("draft.txt"), "invoice? unsure, maybe a quote").unwrap();
    let cache = tempfile::tempdir().unwrap();
    let run = |args: Vec<String>| {
        let mut c = common::grevi(&server);
        c.env("GREVI_CACHE_DIR", cache.path())
            .env_remove("GREVI_NO_CACHE");
        let a = args.clone();
        async move {
            tokio::task::spawn_blocking(move || c.args(a).output().unwrap())
                .await
                .unwrap()
        }
    };
    let dir = d.path().to_string_lossy().to_string();
    let out = run(vec!["--json".into(), "sort".into(), dir.clone()]).await;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["moves"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["data"]["skipped"].as_array().unwrap().len(), 2, "{v}");
    assert!(d.path().join("march.txt").exists(), "dry-run must not move");
    let out = run(vec![
        "--json".into(),
        "sort".into(),
        dir.clone(),
        "--apply".into(),
    ])
    .await;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(d.path().join("Finance/march.txt").exists());
    assert!(
        d.path().join("draft.txt").exists(),
        "a low `any` must not move the file"
    );
    let log = v["data"]["undo_log"].as_str().unwrap().to_string();
    run(vec!["sort".into(), dir, "--undo".into(), log]).await;
    assert!(d.path().join("march.txt").exists());
}
