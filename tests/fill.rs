mod common;

use common::FakeJev;
use serde_json::Value;
use std::{ffi::OsString, os::unix::ffi::OsStringExt, process::Output};
use wiremock::MockServer;

fn run(mut command: assert_cmd::Command, args: &[&str], input: impl AsRef<[u8]>) -> Output {
    command
        .args(args)
        .write_stdin(input.as_ref())
        .output()
        .unwrap()
}

async fn posts(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method == "POST")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

fn envelope(out: &Output, exit: i32) -> Value {
    assert_eq!(
        out.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn sentinel_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script = dir.join("sentinel.sh");
    std::fs::write(&script, "printf ran > \"$1\"\n").unwrap();
    script
}

fn fake() -> FakeJev {
    FakeJev {
        choose: |_, _, options| {
            options
                .iter()
                .find(|s| s.as_str() != "NONE")
                .unwrap()
                .clone()
        },
        noul: |_, _| 0.9,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_round_trips_raw_argv_and_consumes_stdin() {
    let server = common::mock(fake()).await;
    let literal = OsString::from_vec(b"literal\xff'\nline".to_vec());
    let mut dry = common::jevify(&server);
    dry.args([
        "fill",
        "--dry-run",
        "-0",
        "--",
        "sh",
        "tests/bin/argv.sh",
        "0",
    ])
    .arg(&literal)
    .arg("@{-:x}")
    .write_stdin(b"handle\xfe\0");
    let dry = dry.output().unwrap();
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let mut exec = common::jevify(&server);
    exec.args(["fill", "-q", "-0", "--", "sh", "tests/bin/argv.sh", "0"])
        .arg(&literal)
        .arg("@{-:x}")
        .write_stdin(b"handle\xfe\0");
    let exec = exec.output().unwrap();
    assert!(exec.status.success());
    assert_eq!(exec.stdout, b"literal\xff'\nline\0handle\xfe\0stdin:eof\n");
    assert!(exec.stderr.is_empty());
    // The shell reads the printed line back from a script file, as a caller would run it.
    let replay_script = tempfile::tempdir().unwrap().keep().join("replay.sh");
    std::fs::write(&replay_script, &dry.stdout).unwrap();
    let replay = std::process::Command::new("sh")
        .arg(&replay_script)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(replay.stdout, exec.stdout);
    let stderr = String::from_utf8_lossy(&dry.stderr);
    assert!(stderr.lines().all(|line| line.starts_with("jevify fill:")));
    assert!(stderr.contains("\\nline"));
    assert!(stderr.contains("jevify fill: would run "), "{stderr}");
    assert!(!stderr.contains("jevify fill: exec "), "{stderr}");
    let out = run(
        common::jevify(&server),
        &[
            "fill",
            "--json",
            "--dry-run",
            "-0",
            "--",
            "printf",
            "@{-:x}",
        ],
        b"handle\xfe\0",
    );
    assert_eq!(envelope(&out, 6)["error"]["kind"], "cannot_run");
}

#[tokio::test(flavor = "multi_thread")]
async fn context_markers_share_one_post_and_flag_no_removes_argument() {
    use std::os::unix::fs::PermissionsExt;
    let server = common::mock(FakeJev {
        noul: |_, _| 0.1,
        ..fake()
    })
    .await;
    let dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(dir.join("gh"), "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(dir.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = run(
        fixture_command(&server, &dir),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "gh",
            "issue",
            "create",
            "--title",
            "Crash on empty input",
            "--label=@{one:bug|feature|docs:what kind of report is this}",
            "--assignee=@{one:ana|raj|kim:who owns the affected area}",
            "@{flag:--draft:the report lacks steps to reproduce}",
        ],
        "ticket text",
    );
    let value = envelope(&out, 0);
    assert_eq!(value["data"]["argv"][5], "--label=bug");
    assert_eq!(value["data"]["argv"][6], "--assignee=ana");
    assert_eq!(value["data"]["argv"].as_array().unwrap().len(), 7);
    let requests = posts(&server).await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["questions"].as_object().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn flag_band_uses_point_fifteen_and_refuses_to_execute_when_unsure() {
    for (probability, exit, kept) in [
        (0.60, 3, false),
        (0.40, 3, false),
        (0.70, 0, true),
        (0.30, 0, false),
    ] {
        let server = common::mock(move |request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let answers: serde_json::Map<_, _> = body["questions"]
                .as_object()
                .unwrap()
                .keys()
                .map(|key| (key.clone(), serde_json::json!({"noul": probability})))
                .collect();
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"model":"jev-fake", "answers":answers}))
        })
        .await;
        let temp = tempfile::tempdir().unwrap().keep();
        let sentinel = temp.join("sentinel");
        let mut command = common::jevify(&server);
        command
            .args(["fill", "--", "sh"])
            .arg(sentinel_script(&temp))
            .arg(&sentinel)
            .arg("@{flag:--draft:uncertain}");
        let out = command.write_stdin("context").output().unwrap();
        assert_eq!(out.status.code(), Some(exit));
        assert_eq!(sentinel.exists(), exit == 0);
        if exit == 3 {
            assert!(out.stdout.is_empty());
            assert!(String::from_utf8_lossy(&out.stderr).contains("unsure_flag"));
        }
        let out = run(
            common::jevify(&server),
            &[
                "fill",
                "--dry-run",
                "--json",
                "--",
                "printf",
                "@{flag:--draft:uncertain}",
            ],
            "context",
        );
        let value = envelope(&out, exit);
        if exit == 0 {
            assert_eq!(
                value["data"]["argv"].as_array().unwrap().len(),
                if kept { 2 } else { 1 }
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn model_guard_refuses_other_missing_and_mixed_provenance() {
    for model in [
        Some("other-model"),
        Some("other-model, jev-fake"),
        Some("unknown, jev-fake"),
        None,
    ] {
        let server = common::mock(move |request: &wiremock::Request| {
            // Build a complete response explicitly so the absent-model case is truly absent.
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let answers: serde_json::Map<_, _> = body["questions"]
                .as_object()
                .unwrap()
                .keys()
                .map(|key| (key.clone(), serde_json::json!({"noul":0.9})))
                .collect();
            let mut value = serde_json::json!({"answers": answers});
            if let Some(model) = model {
                value["model"] = model.into();
            }
            wiremock::ResponseTemplate::new(200).set_body_json(value)
        })
        .await;
        let temp = tempfile::tempdir().unwrap().keep();
        let sentinel = temp.join("sentinel");
        let mut command = common::jevify(&server);
        command
            .args(["fill", "--", "sh"])
            .arg(sentinel_script(&temp))
            .arg(&sentinel)
            .arg("@{flag:--draft:a}")
            .arg("@{flag:--check:b}");
        let out = command.write_stdin("context").output().unwrap();
        assert_eq!(out.status.code(), Some(4));
        assert!(out.stdout.is_empty());
        assert!(!sentinel.exists());
        let out = run(
            common::jevify(&server),
            &[
                "fill",
                "--dry-run",
                "--json",
                "--",
                "printf",
                "@{flag:--draft:a}",
                "@{flag:--check:b}",
            ],
            "context",
        );
        let value = envelope(&out, 4);
        assert_eq!(value["error"]["kind"], "api_unavailable");
        assert_eq!(
            value["error"]["message"],
            format!(
                "API unavailable: answered by {}, not Jev",
                model.unwrap_or("unknown")
            )
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn several_failures_keep_argv_order_and_error_null() {
    let server = common::mock(FakeJev {
        noul: |_, _| 0.48,
        ..fake()
    })
    .await;
    let dir = tempfile::tempdir().unwrap().keep();
    let context = dir.join("context");
    std::fs::write(&context, "text").unwrap();
    let mut cmd = common::jevify(&server);
    cmd.args(["fill", "--dry-run", "--json", "--context"])
        .arg(&context);
    let out = run(
        cmd,
        &[
            "--",
            "printf",
            "@{-:x}",
            "literal",
            "@{flag:--draft:uncertain}",
        ],
        "",
    );
    let value = envelope(&out, 3);
    assert!(value["error"].is_null());
    assert_eq!(value["data"]["reason"], "no_match");
    assert_eq!(value["data"]["markers"][0]["arg"], 2);
    assert_eq!(value["data"]["markers"][1]["reason"], "unsure_flag");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr)
            .matches("not run:")
            .count(),
        2
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leading_dash_is_filtered_per_marker_and_newlines_are_omitted() {
    let server = common::mock(fake()).await;
    let out = run(
        common::jevify(&server),
        &[
            "fill",
            "--dry-run",
            "--json",
            "-0",
            "--",
            "printf",
            "@{-:x}",
            "--value=@{-:x}",
        ],
        b"-option\0safe\0bad\nline\0",
    );
    let value = envelope(&out, 0);
    assert_eq!(
        value["data"]["argv"],
        serde_json::json!(["printf", "safe", "--value=-option"])
    );
    assert_eq!(value["data"]["markers"][0]["omitted"], 2);
    assert_eq!(value["data"]["markers"][1]["omitted"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn stdin_ownership_files_and_exit_code() {
    let server = common::mock(fake()).await;
    let dir = tempfile::tempdir().unwrap().keep();
    let file = dir.join("input");
    std::fs::write(&file, "x\n").unwrap();
    for (option, marker) in [("--candidates", "@{-:x}"), ("--context", "@{one:x|y:x}")] {
        let mut cmd = common::jevify(&server);
        cmd.args(["fill", "-q", option]).arg(&file);
        let out = run(
            cmd,
            &["--", "sh", "tests/bin/argv.sh", "0", marker],
            "inherited\n",
        );
        assert!(out.status.success());
        assert_eq!(out.stdout, b"x\0stdin:data\n");
        assert!(out.stderr.is_empty());
    }
    let out = run(
        common::jevify(&server),
        &["fill", "-q", "--", "sh", "tests/bin/argv.sh", "1", "@{-:x}"],
        "x\n",
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(out.stdout, b"x\0stdin:eof\n");
    let out = run(
        common::jevify(&server),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "printf",
            "@{-:x}",
            "@{one:x|y:x}",
        ],
        "input",
    );
    envelope(&out, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_and_missing_program_fail_before_requests() {
    let server = common::mock(fake()).await;
    for marker in ["@{widget:x}", "{user}@{host:>8}"] {
        let out = run(
            common::jevify(&server),
            &["fill", "--dry-run", "--json", "--", "printf", marker],
            "input",
        );
        let value = envelope(&out, 2);
        let message = value["error"]["message"].as_str().unwrap();
        assert!(message.contains("nearest kind"));
        assert!(message.contains(&marker.replace("@{", "@@{")));
    }
    let out = run(
        common::jevify(&server),
        &["fill", "--json", "--", "printf", "@{-:x}"],
        "x\n",
    );
    envelope(&out, 2);
    let out = run(
        common::jevify(&server),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "jevify-test-missing-program",
            "@{-:x}",
        ],
        "x\n",
    );
    assert_eq!(envelope(&out, 6)["error"]["kind"], "cannot_run");
    assert!(posts(&server).await.is_empty());
    let out = run(
        common::jevify(&server),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "printf",
            "{user}@@{host:>8}",
            "@{-:x}",
        ],
        "x\n",
    );
    assert_eq!(envelope(&out, 0)["data"]["argv"][1], "{user}@{host:>8}");
}

#[tokio::test(flavor = "multi_thread")]
async fn context_over_budget_makes_no_request() {
    let server = common::mock(fake()).await;
    let out = run(
        common::jevify(&server),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "printf",
            "@{one:x|y:x}",
        ],
        "x".repeat(96_001),
    );
    assert_eq!(envelope(&out, 3)["data"]["reason"], "insufficient_evidence");
    assert!(posts(&server).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn context_questions_are_batched_twenty_per_post() {
    for classifier in [false, true] {
        let server = if classifier {
            common::mock_classifier(fake()).await
        } else {
            common::mock(fake()).await
        };
        let mut cmd = if classifier {
            common::jevify_classifier(&server)
        } else {
            common::jevify(&server)
        };
        cmd.args(["fill", "--dry-run", "--json", "--", "printf"]);
        for i in 0..21 {
            cmd.arg(format!("@{{flag:--flag{i}:condition {i}}}"));
        }
        let out = cmd.write_stdin("context").output().unwrap();
        envelope(&out, 0);
        let requests = posts(&server).await;
        assert_eq!(requests.len(), 2);
        let key = if classifier {
            "dimensions"
        } else {
            "questions"
        };
        let mut sizes: Vec<_> = requests
            .iter()
            .map(|request| request[key].as_object().unwrap().len())
            .collect();
        sizes.sort();
        assert_eq!(sizes, [1, 20]);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn classifier_guard_checks_each_dimension_before_execution() {
    for unknown in [false, true] {
        let server = common::mock_classifier(fake().with_model(move_model)).await;
        // Override just the classify endpoint to omit the model field on the first dimension.
        if unknown {
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .and(wiremock::matchers::path("/v1/classify"))
                .respond_with(|request: &wiremock::Request| {
                    let body: Value = serde_json::from_slice(&request.body).unwrap();
                    let dimensions: serde_json::Map<_, _> = body["dimensions"].as_object().unwrap().iter().enumerate().map(|(i, (id, _))| {
                        let mut value = serde_json::json!({"label":"yes", "confidence":0.9, "scores":{"yes":0.9,"no":0.1}});
                        if i > 0 { value["model"] = "jev-fake".into(); }
                        (id.clone(), value)
                    }).collect();
                    wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"results":[{"dimensions":dimensions}]}))
                }).with_priority(1).mount(&server).await;
        }
        let dir = tempfile::tempdir().unwrap().keep();
        let sentinel = dir.join("sentinel");
        let mut cmd = common::jevify_classifier(&server);
        cmd.args(["fill", "--", "sh"])
            .arg(sentinel_script(&dir))
            .arg(&sentinel)
            .args(["@{flag:--first:first}", "@{flag:--second:second}"]);
        let out = cmd.write_stdin("context").output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(4),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!sentinel.exists());
        assert!(String::from_utf8_lossy(&out.stderr).contains(if unknown {
            "unknown"
        } else {
            "other-model"
        }));
    }
    fn move_model(instructions: &str) -> String {
        if instructions.starts_with("first") {
            "other-model".into()
        } else {
            "jev-fake".into()
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn one_option_limit_on_each_backend() {
    for classifier in [false, true] {
        let server = if classifier {
            common::mock_classifier(fake()).await
        } else {
            common::mock(fake()).await
        };
        let w: usize = if classifier { 99 } else { 200 };
        for count in [w, w + 1] {
            let options = (0..count)
                .map(|i| format!("option{i}"))
                .collect::<Vec<_>>()
                .join("|");
            let marker = format!("@{{one:{options}:choose}}");
            let command = if classifier {
                common::jevify_classifier(&server)
            } else {
                common::jevify(&server)
            };
            let before = posts(&server).await.len();
            let out = run(
                command,
                &["fill", "--dry-run", "--json", "--", "printf", &marker],
                "context",
            );
            let value = envelope(&out, if count == w { 0 } else { 2 });
            if count == w {
                assert_eq!(value["data"]["argv"][1], "option0");
            } else {
                assert!(
                    value["error"]["message"]
                        .as_str()
                        .unwrap()
                        .contains(&count.to_string())
                );
                assert_eq!(posts(&server).await.len(), before);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn listing_capacity_and_three_finalists_on_each_backend() {
    for classifier in [false, true] {
        let server = if classifier {
            common::mock_classifier(fake()).await
        } else {
            common::mock(fake()).await
        };
        let w: usize = if classifier { 99 } else { 200 };
        let f = w * (w / 3);
        for count in [1, 2, w, w + 1, 250, f, f + 1] {
            let command = if classifier {
                common::jevify_classifier(&server)
            } else {
                common::jevify(&server)
            };
            let before = posts(&server).await.len();
            let input = (0..count).map(|i| format!("item{i}\n")).collect::<String>();
            let out = run(
                command,
                &["fill", "--dry-run", "--json", "--", "printf", "@{-:x}"],
                input,
            );
            let value = envelope(&out, if count > f { 6 } else { 0 });
            let requests = posts(&server).await;
            if count > f {
                assert_eq!(value["error"]["kind"], "too_many");
                assert_eq!(requests.len(), before);
            } else {
                assert_eq!(value["data"]["argv"][1], "item0");
                let windows = count.div_ceil(w);
                assert_eq!(requests.len() - before, windows + usize::from(windows > 1));
                if windows > 1 && !classifier {
                    let finals = requests.last().unwrap()["state"]["items"]
                        .as_array()
                        .unwrap();
                    assert_eq!(
                        finals.len(),
                        (0..windows).map(|i| (count - i * w).min(3)).sum::<usize>()
                    );
                }
                assert!(!String::from_utf8_lossy(&out.stderr).contains("finalists per window"));
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ties_none_and_duplicate_evidence_never_execute() {
    // The status line names the rival that decided the abstention, NONE included, and a
    // no_match names the top candidates instead of an empty field.
    for (best, second, none, reason, closest) in [
        (0.45, 0.45, 0.1, "ambiguous", "closest: a (0.45), b (0.45)"),
        (0.5, 0.1, 0.4, "ambiguous", "closest: a (0.50), none (0.40)"),
        (
            0.2,
            0.1,
            0.7,
            "no_match",
            "closest: none (0.70), a (0.20), b (0.10)",
        ),
    ] {
        let server = common::mock(move |request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let answers: serde_json::Map<_, _> = body["questions"].as_object().unwrap().iter().map(|(id, q)| {
                let value = if q["type"] == "noul" { serde_json::json!({"noul":0.9}) } else {
                    serde_json::json!({"choice":"L000","probabilities":{"L000":best,"L001":second,"NONE":none}})
                };
                (id.clone(), value)
            }).collect();
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({"model":"jev-fake", "answers":answers}))
        }).await;
        let dir = tempfile::tempdir().unwrap().keep();
        let sentinel = dir.join("sentinel");
        let mut cmd = common::jevify(&server);
        cmd.args(["fill", "--", "sh"])
            .arg(sentinel_script(&dir))
            .arg(&sentinel)
            .arg("@{-:x}");
        let out = cmd.write_stdin("a\nb\n").output().unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert!(out.stdout.is_empty());
        assert!(!sentinel.exists());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("{reason}; {closest}; candidates 2 of 2")),
            "{stderr}"
        );
        assert!(!stderr.contains("; ; "), "{stderr}");
    }
    // Distinct invalid UTF-8 handles have the same lossy evidence.
    let server = common::mock(fake()).await;
    let out = run(
        common::jevify(&server),
        &[
            "fill",
            "--dry-run",
            "--json",
            "-0",
            "--",
            "printf",
            "@{-:x}",
        ],
        b"\xfe\0\xff\0",
    );
    assert_eq!(envelope(&out, 3)["data"]["reason"], "ambiguous");
    assert!(String::from_utf8_lossy(&out.stderr).contains("2 candidates share the same evidence"));
}

fn branch_fixture(count: usize, twin: bool) -> std::path::PathBuf {
    let mut listing = Vec::new();
    for i in 0..count {
        listing.extend_from_slice(
            format!("refs/heads/b{i}\0\x001700000000\0subject {i}\0\n").as_bytes(),
        );
    }
    if twin {
        listing.extend_from_slice(b"refs/remotes/origin/b0\0\x001700000000\0subject 0\0\n");
    }
    refs_fixture(&listing)
}

/// A fake `git` whose `for-each-ref` prints `listing` and whose `log` prints one commit.
fn refs_fixture(listing: &[u8]) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(dir.join("refs"), listing).unwrap();
    std::fs::write(dir.join("git"), b"#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$FILL_FIXTURE/calls\"\ncase \"$1\" in\nfor-each-ref) cat \"$FILL_FIXTURE/refs\";;\nlog) printf '\\000rich evidence\\000src/code.rs\\000';;\nesac\n").unwrap();
    std::fs::set_permissions(dir.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_branch_listing_says_there_is_nothing_to_choose_from() {
    let server = common::mock(fake()).await;
    let dir = branch_fixture(0, false);
    let out = run(
        fixture_command(&server, &dir),
        &["fill", "--dry-run", "--json", "--", "printf", "@{branch:x}"],
        "",
    );
    let value = envelope(&out, 3);
    assert_eq!(value["data"]["reason"], "no_match");
    assert_eq!(value["data"]["markers"][0]["candidates"], 0);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(
            "not run: arg 2 branch: no_match; no branch to choose from; candidates 0 of 0"
        ),
        "{stderr}"
    );
    assert!(posts(&server).await.is_empty());
}

fn fixture_command(server: &MockServer, dir: &std::path::Path) -> assert_cmd::Command {
    let mut cmd = common::jevify(server);
    cmd.env("FILL_FIXTURE", dir)
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()));
    cmd
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_only_branch_substitutes_the_short_name_that_git_switch_accepts() {
    let server = common::mock(fake()).await;
    let dir = refs_fixture(
        b"refs/remotes/origin/ticket/TPE-791\0\x001700000000\0feat(TPE-791): add Allergy model\0\n",
    );
    let out = run(
        fixture_command(&server, &dir),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "git",
            "switch",
            "@{branch:x}",
        ],
        "",
    );
    let value = envelope(&out, 0);
    assert_eq!(
        value["data"]["argv"],
        serde_json::json!(["git", "switch", "ticket/TPE-791"])
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("'git' 'switch' 'ticket/TPE-791'"),
        "{stderr}"
    );
    assert!(
        stderr.contains("origin/ticket/TPE-791 — feat(TPE-791)"),
        "{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn branch_twins_share_a_candidate_and_decisive_window_skips_enrichment() {
    let server = common::mock(fake()).await;
    let dir = branch_fixture(1, true);
    let out = run(
        fixture_command(&server, &dir),
        &["fill", "--dry-run", "--json", "--", "printf", "@{branch:x}"],
        "",
    );
    let value = envelope(&out, 0);
    assert_eq!(value["data"]["argv"][1], "b0");
    assert_eq!(value["data"]["markers"][0]["candidates"], 1);
    assert_eq!(posts(&server).await.len(), 1);
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert_eq!(calls.lines().count(), 1);
    assert!(calls.starts_with("for-each-ref"));
    // A branch-only run leaves stdin to its command.
    let out = run(
        fixture_command(&server, &dir),
        &[
            "fill",
            "-q",
            "--",
            "sh",
            "tests/bin/argv.sh",
            "0",
            "@{branch:x}",
        ],
        "inherited\n",
    );
    assert_eq!(out.stdout, b"b0\0stdin:data\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn branch_enrichment_only_reads_rank_ordered_finalists_and_listing_is_shared() {
    let server = common::mock(fake()).await;
    let dir = branch_fixture(250, false);
    let out = run(
        fixture_command(&server, &dir),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "printf",
            "@{branch:x}",
            "@{branch:y}",
        ],
        "",
    );
    envelope(&out, 0);
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|line| line.starts_with("for-each-ref"))
            .count(),
        1
    );
    let handles: Vec<_> = calls
        .lines()
        .filter(|line| line.starts_with("log "))
        .map(|line| line.split_whitespace().rev().nth(1).unwrap())
        .collect();
    assert_eq!(handles.len(), 12);
    for handle in &handles {
        assert!(["b0", "b200", "b1", "b201", "b2", "b202"].contains(handle));
    }
    let requests = posts(&server).await;
    let finals: Vec<_> = requests
        .iter()
        .filter(|r| r["state"]["items"].as_array().unwrap().len() == 6)
        .collect();
    assert_eq!(finals.len(), 2);
    for final_request in finals {
        let texts = final_request["state"]["items"].as_array().unwrap();
        for (text, handle) in texts.iter().zip(["b0", "b200", "b1", "b201", "b2", "b202"]) {
            assert!(text.as_str().unwrap().contains(&format!("{handle} —")));
            assert!(text.as_str().unwrap().contains("rich evidence"));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ordered_branch_pool_keeps_newest_capacity_and_enriches_first_twenty_four() {
    let server = common::mock_classifier(fake()).await;
    let capacity = 99 * 33;
    let dir = branch_fixture(capacity + 1, false);
    let mut cmd = common::jevify_classifier(&server);
    cmd.env("FILL_FIXTURE", &dir)
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()));
    let out = run(
        cmd,
        &["fill", "--dry-run", "--json", "--", "printf", "@{branch:x}"],
        "",
    );
    let value = envelope(&out, 0);
    assert_eq!(value["data"]["markers"][0]["candidates"], capacity);
    assert_eq!(value["data"]["markers"][0]["total"], capacity + 1);
    assert!(String::from_utf8_lossy(&out.stderr).contains("candidates 3267 of 3268"));
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    let handles: Vec<_> = calls
        .lines()
        .filter(|line| line.starts_with("log "))
        .map(|line| line.split_whitespace().rev().nth(1).unwrap())
        .collect();
    assert_eq!(
        handles,
        (0..24).map(|i| format!("b{}", i * 99)).collect::<Vec<_>>()
    );
    let requests = posts(&server).await;
    assert_eq!(requests.len(), 34);
    let final_state: Value =
        serde_json::from_str(requests.last().unwrap()["items"][0].as_str().unwrap()).unwrap();
    assert_eq!(final_state["items"].as_array().unwrap().len(), 99);
}

#[tokio::test(flavor = "multi_thread")]
async fn ambiguous_single_branch_window_retries_with_richer_evidence() {
    let server = common::mock(fake().with_probabilities(|_, state, options| {
        if options == ["yes", "no"] {
            return vec![0.9, 0.1];
        }
        let rich = state["items"][0]
            .as_str()
            .unwrap()
            .contains("rich evidence");
        options
            .iter()
            .map(|option| match option.as_str() {
                "L000" => {
                    if rich {
                        0.9
                    } else {
                        0.5
                    }
                }
                "L001" => {
                    if rich {
                        0.05
                    } else {
                        0.4
                    }
                }
                _ => {
                    if rich {
                        0.05
                    } else {
                        0.1
                    }
                }
            })
            .collect()
    }))
    .await;
    let dir = branch_fixture(2, false);
    let out = run(
        fixture_command(&server, &dir),
        &["fill", "--dry-run", "--json", "--", "printf", "@{branch:x}"],
        "",
    );
    assert_eq!(envelope(&out, 0)["data"]["argv"][1], "b0");
    assert_eq!(posts(&server).await.len(), 2);
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|line| line.starts_with("log "))
            .count(),
        2
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn last_window_winner_and_close_rivals_survive_to_finals() {
    let server = common::mock(FakeJev {
        choose: |_, state, options| {
            state["items"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .find(|(_, text)| text.as_str().unwrap().ends_with("item249"))
                .map(|(i, _)| format!("L{i:03}"))
                .unwrap_or_else(|| options[0].clone())
        },
        ..fake()
    })
    .await;
    let input = (0..250).map(|i| format!("item{i}\n")).collect::<String>();
    let out = run(
        common::jevify(&server),
        &["fill", "--dry-run", "--json", "--", "printf", "@{-:x}"],
        &input,
    );
    assert_eq!(envelope(&out, 0)["data"]["argv"][1], "item249");
    assert_eq!(posts(&server).await.len(), 3);

    let server = common::mock(fake().with_probabilities(|_, state, options| {
        if options == ["yes", "no"] {
            return vec![0.9, 0.1];
        }
        options
            .iter()
            .map(|option| {
                if option == "NONE" {
                    return 0.01;
                }
                let index = option[1..].parse::<usize>().unwrap();
                let text = state["items"][index].as_str().unwrap();
                if text.ends_with("item0") {
                    0.45
                } else if text.ends_with("item1") {
                    0.44
                } else {
                    0.001
                }
            })
            .collect()
    }))
    .await;
    let out = run(
        common::jevify(&server),
        &["fill", "--dry-run", "--json", "--", "printf", "@{-:x}"],
        &input,
    );
    assert_eq!(envelope(&out, 3)["data"]["reason"], "ambiguous");
    let requests = posts(&server).await;
    let finals = requests.last().unwrap()["state"]["items"]
        .as_array()
        .unwrap();
    assert!(
        finals
            .iter()
            .any(|s| s.as_str().unwrap().ends_with("item0"))
    );
    assert!(
        finals
            .iter()
            .any(|s| s.as_str().unwrap().ends_with("item1"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn inherited_lister_pipe_fails_without_a_request() {
    use std::os::unix::fs::PermissionsExt;
    let server = common::mock(fake()).await;
    let dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(dir.join("git"), "#!/bin/sh\nsleep 2 &\nexit 0\n").unwrap();
    std::fs::set_permissions(dir.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = run(
        fixture_command(&server, &dir),
        &["fill", "--dry-run", "--json", "--", "printf", "@{branch:x}"],
        "",
    );
    assert_eq!(envelope(&out, 6)["error"]["kind"], "lister_failed");
    assert!(posts(&server).await.is_empty());
}

/// Run with the environment of `configured` and a stdin that stays open with no bytes: any
/// attempted stdin read blocks, and a process that exits within five seconds never read it.
fn run_with_open_stdin(configured: &assert_cmd::Command, args: &[&str]) -> Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_jevify"));
    for (key, value) in configured.get_envs() {
        if let Some(value) = value {
            cmd.env(key, value);
        } else {
            cmd.env_remove(key);
        }
    }
    if let Some(dir) = configured.get_current_dir() {
        cmd.current_dir(dir);
    }
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let _stdin = child.stdin.take().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let exited = loop {
        if child.try_wait().unwrap().is_some() {
            break true;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            break false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let out = child.wait_with_output().unwrap();
    assert!(exited, "the kind check read stdin before validation");
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_kind_does_not_read_stdin_or_invoke_lister() {
    let server = common::mock(fake()).await;
    let dir = branch_fixture(1, false);
    let out = run_with_open_stdin(
        &fixture_command(&server, &dir),
        &[
            "fill",
            "--dry-run",
            "--",
            "printf",
            "@{branch:x}",
            "@{-:x}",
            "{user}@{host:>8}",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("'{user}@@{host:>8}'"));
    assert!(!dir.join("calls").exists());
    assert!(posts(&server).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_input_abstains_without_a_request() {
    let server = common::mock(fake()).await;
    let out = common::jevify(&server)
        .args(["fill", "--dry-run", "--json", "--", "printf", "@{-:x}"])
        .write_stdin("")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["data"]["reason"], "no_match");
    assert!(value["error"].is_null());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no_match; no record to choose from; candidates 0 of 0"),
        "{stderr}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A fake `git` for the `commit` and `file` kinds, and a `gh` that is not logged in. The log
/// holds `count` commits with full 40-hex OIDs, newest first; `ls-files` prints `files`.
const KINDS_GIT: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$FILL_FIXTURE/calls"
case "$1" in
  rev-list) cat "$FILL_FIXTURE/count" ;;
  ls-files) cat "$FILL_FIXTURE/files" ;;
  log)
    if [ "$2" = "-n" ]; then
      n=$3
      total=$(cat "$FILL_FIXTURE/count")
      [ "$n" -gt "$total" ] && n=$total
      i=0
      while [ "$i" -lt "$n" ]; do
        printf '%040d\000made folder moves atomic %s\000' "$i" "$i"
        i=$((i + 1))
      done
    else
      printf 'body of the commit\000\000src/moves.rs\000'
    fi ;;
  *) exit 1 ;;
esac
"#;

fn kinds_fixture(commits: usize, files: &[u8]) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(dir.join("count"), format!("{commits}\n")).unwrap();
    std::fs::write(dir.join("files"), files).unwrap();
    std::fs::write(dir.join("git"), KINDS_GIT).unwrap();
    std::fs::write(
        dir.join("gh"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$FILL_FIXTURE/calls\"\nprintf 'not logged in\\n' >&2\nexit 1\n",
    )
    .unwrap();
    for name in ["git", "gh"] {
        std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

fn calls(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn is_full_oid(value: &Value) -> bool {
    let text = value.as_str().unwrap_or_default();
    text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

#[tokio::test(flavor = "multi_thread")]
async fn commit_resolves_to_a_full_oid_in_two_rounds_above_one_window() {
    let server = common::mock_classifier(fake()).await;
    let dir = kinds_fixture(150, b"");
    let mut cmd = common::jevify_classifier(&server);
    cmd.env("FILL_FIXTURE", &dir)
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()));
    let out = run(
        cmd,
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "git",
            "revert",
            "@{commit:made folder moves atomic}",
        ],
        "",
    );
    let value = envelope(&out, 0);
    assert!(is_full_oid(&value["data"]["argv"][2]), "{value}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("candidates 150, windows 2"), "{stderr}");
    assert!(!stderr.contains(" of "), "{stderr}");
    // Two windows, then one finals request with tier-two evidence for the six finalists only.
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(String::from_utf8_lossy(&requests[2].body).contains("body of the commit"));
    let tier_two: Vec<_> = calls(&dir)
        .into_iter()
        .filter(|line| line.starts_with("log -1 "))
        .collect();
    assert_eq!(tier_two.len(), 6);
    assert!(
        calls(&dir)
            .iter()
            .any(|line| line.starts_with("log -n 3267 "))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn commit_above_capacity_keeps_newest_and_exact_capacity_says_no_of() {
    let server = common::mock_classifier(fake()).await;
    let capacity = 99 * 33;
    for (count, expected) in [
        (
            capacity + 1,
            "candidates 3267 of 3268, newest first, windows 33",
        ),
        (capacity, "candidates 3267, windows 33"),
    ] {
        let dir = kinds_fixture(count, b"");
        let mut cmd = common::jevify_classifier(&server);
        cmd.env("FILL_FIXTURE", &dir)
            .env("PATH", format!("{}:/usr/bin:/bin", dir.display()));
        let before = server.received_requests().await.unwrap().len();
        let out = run(
            cmd,
            &["fill", "--dry-run", "--json", "--", "printf", "@{commit:x}"],
            "",
        );
        let value = envelope(&out, 0);
        assert_eq!(value["data"]["markers"][0]["candidates"], capacity);
        assert_eq!(value["data"]["markers"][0]["total"], count);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(expected), "{stderr}");
        assert_eq!(stderr.contains(" of "), count > capacity);
        assert_eq!(server.received_requests().await.unwrap().len() - before, 34);
    }
}

/// A git work tree with the given files, each holding its own name as content, all tracked.
fn work_tree(files: &[&str]) -> std::path::PathBuf {
    let root = tempfile::tempdir().unwrap().keep();
    for file in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("VISIBLE {file} BODY\n")).unwrap();
    }
    for args in [vec!["init", "-q"], vec!["add", "-A"]] {
        assert!(
            std::process::Command::new("git")
                .current_dir(&root)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    root
}

#[tokio::test(flavor = "multi_thread")]
async fn file_prefix_scopes_the_listing_and_substitutes_relative_to_it() {
    let server = common::mock(FakeJev {
        choose: |_, state, options| {
            let needle = if state["request"].as_str().unwrap().contains("stages") {
                "add.rs"
            } else {
                "app.toml"
            };
            common::option_containing(state, options, needle)
        },
        noul: |_, _| 0.9,
    })
    .await;
    let root = work_tree(&[
        "src/cmd/add.rs",
        "src/cmd/other.rs",
        "conf/app.toml",
        "README",
    ]);
    let mut cmd = common::jevify(&server);
    cmd.current_dir(&root);
    let out = run(
        cmd,
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "printf",
            "src/cmd/@{file:stages hunks}",
            "--config=conf/@{file:the app configuration}",
        ],
        "",
    );
    let value = envelope(&out, 0);
    assert_eq!(
        value["data"]["argv"],
        serde_json::json!(["printf", "src/cmd/add.rs", "--config=conf/app.toml"])
    );
    assert_eq!(value["data"]["markers"][0]["candidates"], 2);
    assert_eq!(value["data"]["markers"][1]["candidates"], 1);
    // A prefix that names no directory fails the lister, before any request.
    let before = posts(&server).await.len();
    let mut cmd = common::jevify(&server);
    cmd.current_dir(&root);
    let out = run(
        cmd,
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "printf",
            "nope/@{file:x}",
        ],
        "",
    );
    assert_eq!(envelope(&out, 6)["error"]["kind"], "lister_failed");
    assert_eq!(posts(&server).await.len(), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn path_handle_with_leading_dash_gets_dot_slash_and_non_utf8_reaches_the_command() {
    let server = common::mock(FakeJev {
        choose: |_, state, options| {
            let needle = if state["request"] == "dash" {
                "-weird"
            } else {
                "caf"
            };
            common::option_containing(state, options, needle)
        },
        noul: |_, _| 0.9,
    })
    .await;
    let dir = kinds_fixture(0, b"-weird\0caf\xe9.txt\0");
    let out = run(
        fixture_command(&server, &dir),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "printf",
            "@{file:dash}",
        ],
        "",
    );
    assert_eq!(envelope(&out, 0)["data"]["argv"][1], "./-weird");
    let helper = format!("{}/tests/bin/argv.sh", env!("CARGO_MANIFEST_DIR"));
    let out = run(
        fixture_command(&server, &dir),
        &["fill", "-q", "--", "sh", &helper, "0", "@{file:the cafe}"],
        "inherited\n",
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"caf\xe9.txt\0stdin:data\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn file_tier_two_carries_meaningful_lines_and_withheld_paths_carry_none() {
    let root = work_tree(&["src/cmd/add.rs", "src/other.rs", ".npmrc"]);
    std::fs::write(
        root.join("src/cmd/add.rs"),
        "use crate::cmd::Outcome;\nuse std::{\n    io::Write,\n    process::Command,\n};\n\n//! STAGES-HUNKS doc line\npub async fn run() {}\n",
    )
    .unwrap();
    std::fs::write(root.join(".npmrc"), "TOKEN=1099\n").unwrap();
    // Round one over names alone is ambiguous or a no_match (low `any`): both reach the finals.
    for request in ["ambiguous-names", "no-match-names"] {
        let server = common::mock(fake().with_probabilities(|_, state, options| {
            let items = state["items"].as_array().unwrap();
            let second_round = items
                .iter()
                .any(|item| item.as_str().unwrap().contains("STAGES-HUNKS"));
            if options == ["yes", "no"] {
                return if second_round || state["request"] == "ambiguous-names" {
                    vec![0.9, 0.1]
                } else {
                    vec![0.1, 0.9]
                };
            }
            options
                .iter()
                .map(|option| {
                    if option == "NONE" {
                        return 0.01;
                    }
                    let text = items[option[1..].parse::<usize>().unwrap()]
                        .as_str()
                        .unwrap();
                    match (
                        second_round,
                        text.contains("add.rs"),
                        text.contains(".npmrc"),
                    ) {
                        (true, true, _) => 0.9,
                        (true, ..) => 0.02,
                        (false, true, _) => 0.45,
                        (false, _, true) => 0.40,
                        _ => 0.1,
                    }
                })
                .collect()
        }))
        .await;
        let mut cmd = common::jevify(&server);
        cmd.current_dir(&root);
        let marker = format!("@{{file:{request}}}");
        let out = run(
            cmd,
            &["fill", "--dry-run", "--json", "--", "printf", &marker],
            "",
        );
        assert_eq!(envelope(&out, 0)["data"]["argv"][1], "src/cmd/add.rs");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2, "{request}");
        let finals = String::from_utf8_lossy(&requests[1].body);
        assert!(finals.contains("STAGES-HUNKS doc line"), "{finals}");
        assert!(finals.contains("pub async fn run()"), "{finals}");
        assert!(!finals.contains("use crate::cmd::Outcome"), "{finals}");
        assert!(!finals.contains("io::Write"), "{finals}");
        assert!(!finals.contains("1099"), "{finals}");
        assert!(finals.contains(".npmrc"), "{finals}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn dir_tier_two_names_children_and_withheld_dirs_carry_none() {
    let root = work_tree(&[
        "evals/why/cargo-build.log",
        "evals/why/pytest.log",
        "evals/report.md",
        "src/lib.rs",
        ".secrets/token.txt",
    ]);
    // Round one over directory names alone is a no_match (low `any`): the finals decide.
    let server = common::mock(fake().with_probabilities(|_, state, options| {
        let items = state["items"].as_array().unwrap();
        let second_round = items
            .iter()
            .any(|item| item.as_str().unwrap().contains("entries:"));
        if options == ["yes", "no"] {
            return if second_round {
                vec![0.9, 0.1]
            } else {
                vec![0.1, 0.9]
            };
        }
        options
            .iter()
            .map(|option| {
                if option == "NONE" {
                    return 0.01;
                }
                let text = items[option[1..].parse::<usize>().unwrap()]
                    .as_str()
                    .unwrap();
                match (
                    second_round,
                    text.contains("pytest.log"),
                    text.contains(".secrets"),
                ) {
                    (true, true, _) => 0.9,
                    (true, ..) => 0.02,
                    (false, _, true) => 0.45,
                    (false, ..) => 0.40,
                }
            })
            .collect()
    }))
    .await;
    let mut cmd = common::jevify(&server);
    cmd.current_dir(&root);
    let out = run(
        cmd,
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "ls",
            "@{dir:the failure logs}",
        ],
        "",
    );
    let value = envelope(&out, 0);
    assert_eq!(value["data"]["argv"][1], "evals/why");
    assert_eq!(value["data"]["markers"][0]["candidates"], 4);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let names = String::from_utf8_lossy(&requests[0].body);
    assert!(!names.contains("entries:"), "{names}");
    let finals = String::from_utf8_lossy(&requests[1].body);
    assert!(
        finals.contains("2 entries: cargo-build.log, pytest.log"),
        "{finals}"
    );
    assert!(finals.contains("report.md, why/"), "{finals}");
    assert!(finals.contains(".secrets"), "{finals}");
    assert!(!finals.contains("token.txt"), "{finals}");
    assert!(!finals.contains("VISIBLE"), "{finals}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("excerpts withheld: 1"), "{stderr}");
}

#[tokio::test(flavor = "multi_thread")]
async fn withheld_excerpts_count_finalists_only_and_never_leave_the_machine() {
    let root = work_tree(&["notes.txt", "other.txt", "third.txt", ".npmrc", ".env"]);
    std::fs::write(root.join(".npmrc"), "TOKEN=1099\n").unwrap();
    std::fs::write(root.join(".env"), "SECRET=zq7mvalue\n").unwrap();
    for (request, withheld) in [("npmrc-finalist", 1), ("plain-finalists", 0)] {
        let server = common::mock(fake().with_probabilities(|_, state, options| {
            if options == ["yes", "no"] {
                return vec![0.9, 0.1];
            }
            let items = state["items"].as_array().unwrap();
            let second_round = items
                .iter()
                .any(|item| item.as_str().unwrap().contains("VISIBLE"));
            let runner_up = if state["request"] == "npmrc-finalist" {
                ".npmrc"
            } else {
                "other.txt"
            };
            options
                .iter()
                .map(|option| {
                    if option == "NONE" {
                        return 0.01;
                    }
                    let text = items[option[1..].parse::<usize>().unwrap()]
                        .as_str()
                        .unwrap();
                    if second_round {
                        if text.contains("notes.txt") {
                            0.9
                        } else {
                            0.02
                        }
                    } else if text.contains("notes.txt") {
                        0.45
                    } else if text.contains(runner_up) {
                        0.40
                    } else if text.contains("other.txt") || text.contains("third.txt") {
                        0.1
                    } else {
                        0.01
                    }
                })
                .collect()
        }))
        .await;
        let mut cmd = common::jevify(&server);
        cmd.current_dir(&root);
        let marker = format!("@{{file:{request}}}");
        let out = run(
            cmd,
            &["fill", "--dry-run", "--json", "--", "printf", &marker],
            "",
        );
        assert_eq!(envelope(&out, 0)["data"]["argv"][1], "notes.txt");
        let stderr = String::from_utf8_lossy(&out.stderr);
        if withheld > 0 {
            assert!(
                stderr.contains(&format!("excerpts withheld: {withheld}")),
                "{stderr}"
            );
        } else {
            assert!(!stderr.contains("excerpts withheld"), "{stderr}");
        }
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        let finals = String::from_utf8_lossy(&requests[1].body);
        assert!(finals.contains("VISIBLE notes.txt BODY"));
        assert_eq!(finals.contains(".npmrc"), withheld > 0);
        for request in &requests {
            let body = String::from_utf8_lossy(&request.body);
            assert!(!body.contains("1099"), "{body}");
            assert!(!body.contains("zq7mvalue"), "{body}");
        }
    }
}

const WIDGET_RECIPE: &str = "{\"kind\":\"widget\",\"list\":[\"sh\",\"-c\",\"printf 'w1 alpha\\\\nw2 beta\\\\n'\"],\"field\":1}\n";

fn with_env(
    mut cmd: assert_cmd::Command,
    key: &str,
    value: &std::path::Path,
) -> assert_cmd::Command {
    cmd.env(key, value);
    cmd
}

fn in_dir(mut cmd: assert_cmd::Command, dir: &std::path::Path) -> assert_cmd::Command {
    cmd.current_dir(dir);
    cmd
}

fn config_with(lines: &str) -> std::path::PathBuf {
    let dir = tempfile::tempdir().unwrap().keep();
    std::fs::write(dir.join("kinds.jsonl"), lines).unwrap();
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn user_recipe_resolves_and_a_shadowing_line_is_recipe_invalid_with_its_number() {
    let server = common::mock(FakeJev {
        choose: |_, state, options| {
            let pick = common::option_containing(state, options, "alpha");
            if pick == "NONE" {
                // The branch listing of the fixture: pick its only candidate.
                options.iter().find(|s| *s != "NONE").unwrap().clone()
            } else {
                pick
            }
        },
        noul: |_, _| 0.9,
    })
    .await;
    let valid = config_with(WIDGET_RECIPE);
    let out = run(
        with_env(common::jevify(&server), "JEVIFY_CONFIG_DIR", &valid),
        &["fill", "--dry-run", "--json", "--", "printf", "@{widget:x}"],
        "",
    );
    assert_eq!(envelope(&out, 0)["data"]["argv"][1], "w1");
    // The nearest-kind suggestion knows the user's recipes.
    let out = run(
        with_env(common::jevify(&server), "JEVIFY_CONFIG_DIR", &valid),
        &["fill", "--dry-run", "--", "printf", "@{widgt:x}"],
        "",
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nearest kind: widget"));

    // The same line named `branch` on line 2: invalid for `widget`, never read for `branch`.
    let dir = branch_fixture(1, false);
    let shadowing = config_with(&format!(
        "{WIDGET_RECIPE}{}",
        WIDGET_RECIPE.replace("\"widget\"", "\"branch\"")
    ));
    let before = posts(&server).await.len();
    let out = run(
        with_env(
            fixture_command(&server, &dir),
            "JEVIFY_CONFIG_DIR",
            &shadowing,
        ),
        &["fill", "--dry-run", "--json", "--", "printf", "@{widget:x}"],
        "",
    );
    let value = envelope(&out, 6);
    assert_eq!(value["error"]["kind"], "recipe_invalid");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("line 2"),
        "{value}"
    );
    assert_eq!(posts(&server).await.len(), before);
    assert!(!dir.join("calls").exists());
    let out = run(
        with_env(
            fixture_command(&server, &dir),
            "JEVIFY_CONFIG_DIR",
            &shadowing,
        ),
        &["fill", "--dry-run", "--json", "--", "printf", "@{branch:x}"],
        "",
    );
    assert_eq!(envelope(&out, 0)["data"]["argv"][1], "b0");

    // A kinds.jsonl in the working directory is never read.
    let out = run(
        in_dir(common::jevify(&server), &valid),
        &["fill", "--dry-run", "--", "printf", "@{widget:x}"],
        "",
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown kind 'widget'") && stderr.contains("nearest kind"));
}

#[tokio::test(flavor = "multi_thread")]
async fn user_recipe_kind_check_runs_before_stdin_and_listers() {
    let server = common::mock(fake()).await;
    let dir = branch_fixture(1, false);
    // A valid recipe passes the check; the unknown kind after it fails before stdin is read.
    let valid = config_with(WIDGET_RECIPE);
    let mut configured = fixture_command(&server, &dir);
    configured.env("JEVIFY_CONFIG_DIR", &valid);
    let out = run_with_open_stdin(
        &configured,
        &[
            "fill",
            "--dry-run",
            "--",
            "printf",
            "@{widget:x}",
            "@{-:x}",
            "{user}@{host:>8}",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown kind 'host'"), "{stderr}");
    // A bad recipe file is recipe_invalid at the check, before stdin and before any lister.
    let invalid = config_with("{\"kind\":\"widget\"}\n");
    let mut configured = fixture_command(&server, &dir);
    configured.env("JEVIFY_CONFIG_DIR", &invalid);
    let out = run_with_open_stdin(
        &configured,
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "printf",
            "@{branch:x}",
            "@{-:x}",
            "@{widget:x}",
        ],
    );
    let value = envelope(&out, 6);
    assert_eq!(value["error"]["kind"], "recipe_invalid");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("line 1")
    );
    assert!(!dir.join("calls").exists());
    assert!(posts(&server).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn gh_not_logged_in_is_lister_failed_with_its_text_and_no_request() {
    let server = common::mock(fake()).await;
    let dir = kinds_fixture(0, b"");
    let out = run(
        fixture_command(&server, &dir),
        &["fill", "--dry-run", "--json", "--", "printf", "@{pr:x}"],
        "",
    );
    let value = envelope(&out, 6);
    assert_eq!(value["error"]["kind"], "lister_failed");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not logged in"),
        "{value}"
    );
    assert!(posts(&server).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn too_many_files_names_both_ways_to_narrow_and_sends_nothing() {
    let server = common::mock_classifier(fake()).await;
    let files: Vec<u8> = (0..99 * 33 + 1)
        .flat_map(|i| format!("f{i}\0").into_bytes())
        .collect();
    let dir = kinds_fixture(0, &files);
    let mut cmd = common::jevify_classifier(&server);
    cmd.env("FILL_FIXTURE", &dir)
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()));
    let out = run(
        cmd,
        &["fill", "--dry-run", "--json", "--", "printf", "@{file:x}"],
        "",
    );
    let value = envelope(&out, 6);
    assert_eq!(value["error"]["kind"], "too_many");
    let hint = value["error"]["hint"].as_str().unwrap();
    assert!(hint.contains("prefix") && hint.contains("@{-:"), "{hint}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_lists_the_executables_of_the_path() {
    use std::os::unix::fs::PermissionsExt;
    let server = common::mock(FakeJev {
        choose: |_, state, options| common::option_containing(state, options, "beta-tool"),
        noul: |_, _| 0.9,
    })
    .await;
    let dir = tempfile::tempdir().unwrap().keep();
    for name in ["alpha-tool", "beta-tool"] {
        std::fs::write(dir.join(name), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut cmd = common::jevify(&server);
    cmd.env("PATH", &dir);
    let out = run(
        cmd,
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "/bin/echo",
            "@{tool:x}",
        ],
        "",
    );
    let value = envelope(&out, 0);
    assert_eq!(value["data"]["argv"][1], "beta-tool");
    assert_eq!(value["data"]["markers"][0]["candidates"], 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_marker_in_argv0_is_refused_with_pick_and_a_split_command_keeps_its_message() {
    let server = common::mock(fake()).await;
    let out = run(
        common::jevify(&server),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "@{tool:the GitHub command line}",
            "--version",
        ],
        "",
    );
    let value = envelope(&out, 2);
    let message = value["error"]["message"].as_str().unwrap();
    assert!(message.contains("the command must be literal"), "{message}");
    assert!(
        message.contains("jevify pick --from tool 'the GitHub command line'"),
        "{message}"
    );
    assert!(!message.contains("separate arguments"), "{message}");
    let out = run(
        common::jevify(&server),
        &[
            "fill",
            "--dry-run",
            "--json",
            "--",
            "git switch",
            "@{file:x}",
        ],
        "",
    );
    let message = envelope(&out, 2)["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        message.contains("pass the command as separate arguments"),
        "{message}"
    );
    assert!(posts(&server).await.is_empty());
}
