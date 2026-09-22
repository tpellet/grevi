mod common;

fn fake_branches(count: usize) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap().keep();
    std::fs::write(
        root.join("git"),
        format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$PICK_GIT_LOG"
case "$1" in
  for-each-ref)
    i=0
    while [ "$i" -lt {count} ]; do
      printf 'refs/heads/branch-%s\000\0001700000000\000subject\000\n' "$i"
      i=$((i + 1))
    done ;;
  log) printf '\000richer evidence\000\000src/file\000' ;;
  *) exit 1 ;;
esac
"#
        ),
    )
    .unwrap();
    std::fs::set_permissions(root.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    root
}

fn branch_command(
    server: &wiremock::MockServer,
    root: &std::path::Path,
    classifier: bool,
) -> assert_cmd::Command {
    let mut cmd = if classifier {
        common::jevify_classifier(server)
    } else {
        common::jevify(server)
    };
    cmd.env("PATH", root)
        .env("PICK_GIT_LOG", root.join("calls"));
    cmd
}

#[tokio::test(flavor = "multi_thread")]
async fn from_branch_decisive_is_one_request_and_ignores_stdin() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "branch-3"),
        noul: |_, _| 0.9,
    })
    .await;
    let root = fake_branches(5);
    for machine in [false, true] {
        let mut cmd = branch_command(&server, &root, false);
        if machine {
            cmd.arg("--json");
        }
        let out = cmd
            .args(["pick", "--from", "branch", "x"])
            .write_stdin("STDIN_SENTINEL")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        if machine {
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(v["data"]["matches"][0]["text"], "branch-3");
            assert_eq!(v["meta"]["requests"], 1);
        } else {
            assert_eq!(out.stdout, b"branch-3\n");
        }
    }
    let calls = std::fs::read_to_string(root.join("calls")).unwrap();
    assert_eq!(calls.lines().count(), 2);
    assert!(!calls.lines().any(|s| s.starts_with("log ")));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|r| !String::from_utf8_lossy(&r.body).contains("STDIN_SENTINEL"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn from_branch_ratio_retries_and_abstention_reasons() {
    for enriched_wins in [false, true] {
        let server = common::mock(
            FakeJev {
                choose: |_, _, _| "L000".into(),
                noul: |_, _| 0.9,
            }
            .with_probabilities(|_, state, options| {
                if options == ["yes", "no"] {
                    return vec![0.9, 0.1];
                }
                let wins =
                    state["request"] == "resolve" && state.to_string().contains("richer evidence");
                options
                    .iter()
                    .map(|o| match o.as_str() {
                        "L000" if wins => 0.9,
                        _ if wins => 0.1 / (options.len() - 1) as f64,
                        "L000" | "L001" => 0.45,
                        _ => 0.025,
                    })
                    .collect()
            }),
        )
        .await;
        let root = fake_branches(5);
        let out = branch_command(&server, &root, false)
            .args([
                "--json",
                "pick",
                "--from",
                "branch",
                if enriched_wins { "resolve" } else { "tie" },
            ])
            .output()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            out.status.code(),
            Some(if enriched_wins { 0 } else { 3 }),
            "{v}"
        );
        assert_eq!(v["meta"]["requests"], 2);
        if !enriched_wins {
            assert_eq!(v["data"]["reason"], "ambiguous");
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(stderr.contains("branch-0") && stderr.contains("branch-1"));
        }
        assert_eq!(
            std::fs::read_to_string(root.join("calls"))
                .unwrap()
                .lines()
                .filter(|s| s.starts_with("log "))
                .count(),
            3
        );
    }
    let server = common::mock(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.1,
    })
    .await;
    for count in [0, 5] {
        let root = fake_branches(count);
        let out = branch_command(&server, &root, false)
            .args(["--json", "pick", "--from", "branch", "x"])
            .output()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(v["data"]["reason"], "no_match");
        assert_eq!(v["meta"]["requests"], usize::from(count != 0));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn from_branch_classifier_volume_enriches_only_first_24_finalists() {
    let server = common::mock_classifier(FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |_, _| 0.9,
    })
    .await;
    for (count, kept, n) in [(4000, 4000_usize, 2), (9802, 9801, 1)] {
        let root = fake_branches(count);
        let out = branch_command(&server, &root, true)
            .args(["--json", "pick", "--from", "branch", "x"])
            .output()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["data"]["candidates"], kept);
        assert_eq!(v["data"]["finalists_per_window"], n);
        assert_eq!(v["meta"]["requests"], kept.div_ceil(99) + 1);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(&format!("finalists per window: {n}")));
        if count == 9802 {
            assert!(stderr.contains("candidates 9801 of 9802, newest first"));
        }
        let calls = std::fs::read_to_string(root.join("calls")).unwrap();
        let logs: Vec<_> = calls.lines().filter(|s| s.starts_with("log ")).collect();
        assert_eq!(logs.len(), 24);
        for (i, log) in logs.iter().enumerate() {
            assert!(log.ends_with(&format!("branch-{} --", i * 99)), "{log}");
        }
    }
}

#[test]
fn from_kind_validation_and_lister_failure() {
    let root = tempfile::tempdir().unwrap().keep();
    for (kind, exit, message) in [
        ("branc", 2, "branch"),
        ("-", 2, "default source"),
        ("branch", 6, "lister_failed"),
    ] {
        let out = common::bin()
            .current_dir(&root)
            .env("GIT_CEILING_DIRECTORIES", &root)
            .args(["--json", "pick", "--from", kind, "x"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(exit));
        assert!(String::from_utf8_lossy(&out.stdout).contains(message));
    }
    for flag in ["--files", "--index", "-0", "--para"] {
        common::bin()
            .args(["pick", "--from", "branch", flag, "x"])
            .assert()
            .code(2);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn from_branch_top_handles_and_non_utf8_are_preserved() {
    let server = common::mock(
        FakeJev {
            choose: |_, _, _| "L000".into(),
            noul: |_, _| 0.9,
        }
        .with_probabilities(|_, _, options| {
            if options == ["yes", "no"] {
                return vec![0.9, 0.1];
            }
            options
                .iter()
                .map(|o| match o.as_str() {
                    "L000" => 0.7,
                    "L001" => 0.2,
                    _ => 0.1,
                })
                .collect()
        }),
    )
    .await;
    let root = fake_branches(2);
    let out = branch_command(&server, &root, false)
        .args(["pick", "--from", "branch", "-n", "2", "x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"branch-0\nbranch-1\n");
    std::fs::write(
        root.join("git"),
        "#!/bin/sh\nprintf 'refs/heads/raw-\\377\\000\\0001700000000\\000subject\\000\\n'\n",
    )
    .unwrap();
    for machine in [false, true] {
        let mut cmd = branch_command(&server, &root, false);
        if machine {
            cmd.arg("--json");
        }
        let out = cmd
            .args(["pick", "--from", "branch", "x"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        if machine {
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(v["data"]["matches"][0]["text"], "raw-\u{fffd}");
            assert_eq!(v["data"]["matches"][0]["lossy"], true);
            assert_eq!(v["data"]["matches"][0]["ordinal"], 1);
        } else {
            assert_eq!(out.stdout, b"raw-\xff\n");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn from_branch_remote_twin_uses_local_handle() {
    let root = tempfile::tempdir().unwrap().keep();
    for args in [
        vec!["init", "-q"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "--no-gpg-sign",
            "-qm",
            "subject",
        ],
        vec!["branch", "release"],
        vec!["update-ref", "refs/remotes/origin/release", "HEAD"],
    ] {
        assert!(
            std::process::Command::new("git")
                .current_dir(&root)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "release"),
        noul: |_, _| 0.9,
    })
    .await;
    let out = common::jevify(&server)
        .current_dir(root)
        .args(["pick", "--from", "branch", "release"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"release\n");
    assert!(
        !server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| String::from_utf8_lossy(&r.body).contains("origin/release"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn classifier_capacity_is_too_many_before_any_request() {
    let server = common::mock_classifier(FakeJev {
        choose: |_, _, _| "NONE".into(),
        noul: |_, _| 0.9,
    })
    .await;
    let input = (0..9802).map(|i| format!("item {i}\n")).collect::<String>();
    let mut cmd = common::jevify_classifier(&server);
    let out = tokio::task::spawn_blocking(move || {
        cmd.args(["--json", "pick", "item"])
            .write_stdin(input)
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(6));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["error"]["kind"], "too_many");
    let hint = value["error"]["hint"].as_str().unwrap();
    for way in ["grep", "head", "prefix"] {
        assert!(hint.contains(way));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn stdin_pick_keeps_its_non_ratio_rule_and_top_three() {
    let server = common::mock(
        FakeJev {
            choose: |_, _, _| "L000".into(),
            noul: |_, _| 0.9,
        }
        .with_probabilities(|_, _, options| {
            if options == ["yes", "no"] {
                return vec![0.9, 0.1];
            }
            options
                .iter()
                .map(|o| match o.as_str() {
                    "L000" => 0.4,
                    "L001" => 0.3,
                    "L002" => 0.2,
                    _ => 0.1,
                })
                .collect()
        }),
    )
    .await;
    let mut cmd = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        cmd.args(["pick", "item", "-n", "3"])
            .write_stdin("a\nb\nc\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"a\nb\nc\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn files_keep_more_than_24_finalists_and_always_use_round_two() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| option_containing(s, o, "file-1799"),
        noul: |_, _| 0.9,
    })
    .await;
    let mut cmd = common::jevify(&server);
    let input = (0..1800).map(|i| format!("file-{i}\n")).collect::<String>();
    let out = tokio::task::spawn_blocking(move || {
        cmd.args(["pick", "--files", "last"])
            .write_stdin(input)
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"file-1799\n");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 10);
    let body: serde_json::Value = serde_json::from_slice(&requests.last().unwrap().body).unwrap();
    assert_eq!(body["state"]["items"].as_array().unwrap().len(), 27);
}

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
