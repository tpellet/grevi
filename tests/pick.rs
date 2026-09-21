mod common;

#[tokio::test]
async fn malformed_choice_labels_return_a_protocol_envelope() {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    for label in ["", "é", "X000", "L999"] {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"answers":{"any":{"noul":0.9},"pick":{"choice":label,"probabilities":{label:0.9,"NONE":0.1}}}}))).mount(&server).await;
        let output = tokio::task::spawn_blocking(move || {
            common::jevify(&server)
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
async fn selected_records_preserve_bytes_and_split_terminators() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    for (flag, input, expected) in [
        (
            None,
            b"other\n\x1b[31minvoice\x1b[0m\n".as_slice(),
            b"\x1b[31minvoice\x1b[0m\n".as_slice(),
        ),
        (None, b"other\r\ninvoice\r\n", b"invoice\r\n"),
        (None, b"other\ninvoice\xff\n", b"invoice\xff\n"),
        (None, b"other\ninvoice", b"invoice"),
        (Some("-0"), b"other\0invoice\0", b"invoice\0"),
        (
            Some("--para"),
            b"other\n\ninvoice\ncontinued\n\n",
            b"invoice\ncontinued\n\n",
        ),
    ] {
        let mut cmd = common::jevify(&server);
        cmd.args(["pick", "bill"]);
        if let Some(flag) = flag {
            cmd.arg(flag);
        }
        let out = tokio::task::spawn_blocking(move || cmd.write_stdin(input).output().unwrap())
            .await
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(out.stdout, expected);
    }
    let mut cmd = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        cmd.args(["--json", "pick", "bill"])
            .write_stdin(b"other\ninvoice\xff\n".as_slice())
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["data"]["matches"][0]["text"], "invoice\u{fffd}\n");
    assert_eq!(value["data"]["matches"][0]["lossy"], true);
    assert_eq!(value["data"]["matches"][0]["ordinal"], 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn three_best_index_blank_limit_and_no_saved_input() {
    let server = common::mock(
        FakeJev {
            choose: |_, _, _| "L000".into(),
            noul: |_, _| 0.9,
        }
        .with_probabilities(|_, _, options| {
            options
                .iter()
                .map(|o| match o.as_str() {
                    "L000" => 0.4,
                    "L001" => 0.3,
                    "L002" => 0.2,
                    "NONE" => 0.1,
                    "yes" => 0.9,
                    _ => 0.1,
                })
                .collect()
        }),
    )
    .await;
    let root = tempfile::tempdir().unwrap().keep();
    for (args, input, code, expected) in [
        (
            vec!["pick", "-n", "3", "x"],
            "a\nb\nc\n".to_string(),
            0,
            "a\nb\nc\n",
        ),
        (
            vec!["pick", "--index", "x"],
            "\na\nb\nc\n".to_string(),
            0,
            "2\n",
        ),
        (vec!["pick", "x"], " \n\t\n".to_string(), 6, ""),
        (
            vec!["pick", "x"],
            (0..20_001).map(|i| format!("{i}\n")).collect(),
            6,
            "",
        ),
    ] {
        let mut cmd = common::jevify(&server);
        cmd.env("JEVIFY_CACHE_DIR", &root);
        let out = tokio::task::spawn_blocking(move || {
            cmd.args(args).write_stdin(input).output().unwrap()
        })
        .await
        .unwrap();
        assert_eq!(out.status.code(), Some(code));
        assert_eq!(out.stdout, expected.as_bytes());
    }
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn nul_file_paths_resolve_relative_names_and_withhold_npmrc() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    let root = tempfile::tempdir().unwrap().keep();
    std::fs::write(root.join("invoice.txt"), "VISIBLE_FILE_BODY").unwrap();
    std::fs::write(root.join(".npmrc"), "TOKEN=1099").unwrap();
    let mut cmd = common::jevify(&server);
    cmd.current_dir(root);
    let out = tokio::task::spawn_blocking(move || {
        cmd.args(["pick", "-0", "--files", "bill"])
            .write_stdin(b"./invoice.txt\0.npmrc\0".as_slice())
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"./invoice.txt\0");
    assert!(String::from_utf8_lossy(&out.stderr).contains("excerpts withheld: 1"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let first = String::from_utf8_lossy(&requests[0].body);
    let second = String::from_utf8_lossy(&requests[1].body);
    assert!(first.contains(".npmrc"));
    assert!(!first.contains("VISIBLE_FILE_BODY"));
    assert!(second.contains("VISIBLE_FILE_BODY"));
    assert!(second.contains(".npmrc"));
    assert!(
        requests
            .iter()
            .all(|r| !String::from_utf8_lossy(&r.body).contains("TOKEN=1099"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_file_shortlist_abstains_without_a_second_request() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"answers":{"any":{"noul":0.9},"pick":{"choice":"NONE","probabilities":{"L000":0.0,"NONE":1.0}}}}))).mount(&server).await;
    let mut cmd = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        cmd.args(["--json", "pick", "--files", "x"])
            .write_stdin("a.txt\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["data"]["any"], 0.0);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn twins_that_split_choice_mass_abstain() {
    let server = common::mock(
        FakeJev {
            choose: |_, _, _| "NONE".into(),
            noul: |_, _| 0.9,
        }
        .with_probabilities(|_, _, options| {
            options
                .iter()
                .map(|option| match option.as_str() {
                    "NONE" => 0.4,
                    "yes" => 0.9,
                    "no" => 0.1,
                    _ => 0.3,
                })
                .collect()
        }),
    )
    .await;
    let mut cmd = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        cmd.args(["pick", "invoice"])
            .write_stdin("invoice-a\ninvoice-b\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn prints_matching_line_raw() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "invoice"),
        noul: |_, _| 0.9,
    })
    .await;
    let mut c = common::jevify(&server);
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
    let mut c = common::jevify(&server);
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
    let mut c = common::jevify(&server);
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
    let mut c = common::jevify(&server);
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
    let mut c = common::jevify(&server);
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
            assert!(!s.to_string().contains("TOKEN=1099"));
            if items.len() == 6 {
                assert!(s.to_string().contains(".env"));
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
    let dir = tempfile::Builder::new()
        .prefix("jevify-pick-")
        .tempdir()
        .unwrap();
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
    let mut c = common::jevify(&server);
    let input = [
        "document(3).txt",
        "notes.txt",
        "photo.txt",
        "report.txt",
        "zeta.txt",
        ".env",
    ]
    .iter()
    .map(|name| format!("{root}/{name}\n"))
    .collect::<String>();
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "pick", "--files", "the tax form"])
            .write_stdin(input)
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
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !String::from_utf8_lossy(&request.body).contains("TOKEN=1099"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn files_mode_rejects_index_and_empty_input_and_abstains_honestly() {
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.05,
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();
    let run = |args: Vec<String>, input: &str| {
        let mut c = common::jevify(&server);
        c.args(args).write_stdin(input).output().unwrap()
    };
    let v = |o: &std::process::Output| -> serde_json::Value {
        serde_json::from_slice(&o.stdout).unwrap()
    };
    let args = |extra: &[&str]| -> Vec<String> {
        let mut a = vec!["--json".to_string(), "pick".into(), "--files".into()];
        a.extend(extra.iter().map(|s| s.to_string()));
        a
    };
    let out = run(args(&["--index", "x"]), "a.txt\n");
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v(&out)["error"]["kind"], "usage");
    // No piped paths: nothing to choose from, even if files exist on disk.
    std::fs::write(dir.path().join(".env"), "x").unwrap();
    let out = run(args(&["x"]), "");
    assert_eq!(out.status.code(), Some(6));
    assert_eq!(v(&out)["error"]["kind"], "empty_input");
    assert_eq!(v(&out)["meta"]["requests"], 0);
    std::fs::write(dir.path().join("a.txt"), "x").unwrap();
    let out = run(args(&["a spaceship"]), &format!("{root}/a.txt\n"));
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(v(&out)["data"]["matches"], serde_json::json!([]));
    let out = run(
        vec![
            "--json".into(),
            "pick".into(),
            "--files".into(),
            format!("{root}/missing"),
            "x".into(),
        ],
        "a.txt\n",
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(v(&out)["error"]["kind"], "usage");
}
