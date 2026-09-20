mod common;

#[tokio::test]
async fn malformed_choice_labels_return_a_protocol_envelope() {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    for label in ["", "é", "X000", "L999"] {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"answers":{"any":{"noul":0.9},"pick":{"choice":label,"probabilities":{label:0.9,"NONE":0.1}}}}))).mount(&server).await;
        let output = tokio::task::spawn_blocking(move || {
            common::grevi(&server)
                .args(["--json", "pick", "match"])
                .write_stdin("item\n")
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(output.status.code(), Some(4));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["kind"], "api_protocol");
    }
}
use common::{FakeJev, option_containing};

#[tokio::test(flavor = "multi_thread")]
async fn prints_matching_line_raw() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::grevi(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["pick", "the bill"])
            .write_stdin("notes.txt\ninvoice-march.pdf\nphoto.jpg\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "invoice-march.pdf\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn abstains_with_exit_3() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.05,
    })
    .await;
    let mut c = common::grevi(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["pick", "a spaceship"])
            .write_stdin("a\nb\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn top_n_prints_only_lines_that_beat_none() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::grevi(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["pick", "-n", "3", "the bill"])
            .write_stdin("notes.txt\ninvoice-march.pdf\nphoto.jpg\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "invoice-march.pdf\n"
    );
}

// Blank lines take no window slot and a repeated line is sent once, but `line` still counts
// original stdin lines: this test fails if the `kept[c.index]` remap is transcribed as the old
// `c.index + 1`, and if the duplicate `notes.txt` reaches the model as a third item.
#[tokio::test(flavor = "multi_thread")]
async fn blank_and_duplicate_lines_are_skipped_but_line_numbers_are_original() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| {
            assert_eq!(s["items"].as_array().unwrap().len(), 2, "duplicate sent");
            option_containing(s, o, "invoice")
        },
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::grevi(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "pick", "--index", "the bill"])
            .write_stdin("notes.txt\n\ninvoice-march.pdf\nnotes.txt\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["matches"][0]["line"], 3, "{v}");
    assert_eq!(v["data"]["matches"][0]["text"], "invoice-march.pdf");
}

#[tokio::test(flavor = "multi_thread")]
async fn abstains_when_none_wins_the_choice() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::grevi(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["pick", "a spaceship"])
            .write_stdin("a\nb\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(3));
}

// `--files`: round one carries path names only; the beginning of a file's content reaches the
// model in the finals round alone. The name `document(3).txt` says nothing, its content does.
#[tokio::test(flavor = "multi_thread")]
async fn files_are_ranked_by_name_first_and_found_by_content_in_the_finals() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| {
            let items = s["items"].as_array().unwrap();
            if items.len() == 5 {
                assert!(
                    !s.to_string().contains("1099"),
                    "file content left the machine in round one"
                );
            } else {
                assert_eq!(items.len(), 3, "finals carry the finalists only");
            }
            option_containing(s, o, "1099")
        },
        noul: |_, _| 0.9,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    for (name, body) in [
        ("document(3).txt", "Form 1099-INT interest income"),
        ("notes.txt", "buy milk"),
        ("photo.txt", "a beach"),
        ("report.txt", "quarterly numbers"),
        ("zeta.txt", "last"),
        (".env", "TOKEN=1099"),
    ] {
        std::fs::write(dir.path().join(name), body).unwrap();
    }
    let root = dir.path().to_str().unwrap().to_string();
    let mut c = common::grevi(&server);
    let arg = root.clone();
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "pick", "--files", &arg, "the tax form"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["source"], "files", "{v}");
    assert_eq!(
        v["data"]["matches"][0]["text"],
        format!("{root}/document(3).txt")
    );
    assert_eq!(v["meta"]["requests"], 2, "two rounds, no more: {v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn files_mode_rejects_index_and_an_empty_directory_and_abstains_honestly() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.05,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();
    let run = |args: Vec<String>| {
        let mut c = common::grevi(&server);
        c.args(args).output().unwrap()
    };
    let v = |o: &std::process::Output| -> serde_json::Value {
        serde_json::from_slice(&o.stdout).unwrap()
    };
    let args = |extra: &[&str]| -> Vec<String> {
        let mut a = vec![
            "--json".to_string(),
            "pick".into(),
            "--files".into(),
            root.clone(),
        ];
        a.extend(extra.iter().map(|s| s.to_string()));
        a
    };
    let out = run(args(&["--index", "x"]));
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v(&out)["error"]["kind"], "usage");
    // Only a hidden file: nothing to choose from, and no request is made.
    std::fs::write(dir.path().join(".env"), "x").unwrap();
    let out = run(args(&["x"]));
    assert_eq!(out.status.code(), Some(6));
    assert_eq!(v(&out)["error"]["kind"], "empty_input");
    assert_eq!(v(&out)["meta"]["requests"], 0);
    std::fs::write(dir.path().join("a.txt"), "x").unwrap();
    let out = run(args(&["a spaceship"]));
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(v(&out)["data"]["matches"], serde_json::json!([]));
    let out = run(vec![
        "--json".into(),
        "pick".into(),
        "--files".into(),
        format!("{root}/missing"),
        "x".into(),
    ]);
    assert_eq!(out.status.code(), Some(6));
}
