//! `meta.decision`: verb, backend, requested and answering model, threshold and the gate
//! scores of every decision, on every verb.
mod common;

use common::FakeJev;
use serde_json::Value;
use std::process::Command as P;

fn envelope(out: &std::process::Output) -> Value {
    let parsed = serde_json::from_slice(&out.stdout);
    assert!(
        parsed.is_ok(),
        "no envelope: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    parsed.unwrap()
}

/// The fields every verb carries; returns the gates.
fn decision(v: &Value, verb: &str, backend: &str) -> Vec<Value> {
    let d = &v["meta"]["decision"];
    assert_eq!(d["verb"], verb, "{v}");
    assert_eq!(d["backend"], backend, "{v}");
    assert_eq!(d["backend"], v["meta"]["backend"], "{v}");
    assert_eq!(d["threshold"], 0.5, "{v}");
    assert_eq!(d["threshold"], v["meta"]["threshold"], "{v}");
    if backend == "typesafe" {
        assert_eq!(d["model"]["requested"], "jev-1.13.0", "{v}");
    } else {
        assert!(d["model"]["requested"].is_null(), "{v}");
    }
    assert_eq!(d["model"]["answering"], "jev-fake", "{v}");
    assert_eq!(d["model"]["answering"], v["meta"]["model"], "{v}");
    assert!(d["round_one"].is_array(), "{v}");
    let gates = d["gates"].as_array().unwrap().clone();
    for gate in &gates {
        for score in ["best", "next", "none", "any", "fails"] {
            assert!(gate[score].is_null() || gate[score].is_number(), "{v}");
        }
    }
    gates
}

fn noul_gate(p: f64) -> Value {
    serde_json::json!({ "best": null, "next": null, "none": null, "any": p, "fails": null })
}

#[tokio::test(flavor = "multi_thread")]
async fn is_records_one_noul_gate_per_statement() {
    let server = common::mock(FakeJev {
        choose: |_, _, o| o[0].clone(),
        noul: |i, _| if i.contains("first") { 0.81 } else { 0.2 },
    })
    .await;
    let mut c = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "is", "first"])
            .write_stdin("text")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v = envelope(&out);
    assert_eq!(decision(&v, "is", "typesafe"), vec![noul_gate(0.81)]);
    // No tournament: a yes/no verb records no round one.
    assert_eq!(v["meta"]["decision"]["round_one"], serde_json::json!([]));
    let mut c = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "is", "first", "second"])
            .write_stdin("text")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v = envelope(&out);
    assert_eq!(
        decision(&v, "is", "typesafe"),
        vec![noul_gate(0.81), noul_gate(0.2)]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn filter_records_one_three_way_gate_per_judged_record() {
    let server = common::mock_classifier(FakeJev {
        choose: |_, state, options| {
            let side = match state.as_str() {
                Some("no") => "does not hold",
                Some("silent") => "does not say",
                _ => "statement holds",
            };
            options
                .iter()
                .find(|option| option.ends_with(side))
                .cloned()
                .expect("one of the three filter options ends with the side")
        },
        noul: |_, _| unreachable!(),
    })
    .await;
    let mut c = common::jevify_classifier(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "filter", "x"])
            .write_stdin("yes\nno\nyes\nsilent\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v = envelope(&out);
    // A duplicate record is judged once: three gates for four records. `any` is P(holds),
    // `fails` is P(does not hold) and `none` is P(the record does not say); the fake gives 0.9
    // to its pick and 0.05 to the rest.
    let gates = decision(&v, "filter", "classifier");
    assert_eq!(gates.len(), 3, "{v}");
    let sides = [(0.9, 0.05, 0.05), (0.05, 0.9, 0.05), (0.05, 0.05, 0.9)];
    for (gate, (holds, fails, silent)) in gates.iter().zip(sides) {
        assert!((gate["any"].as_f64().unwrap() - holds).abs() < 1e-9, "{v}");
        assert!(
            (gate["fails"].as_f64().unwrap() - fails).abs() < 1e-9,
            "{v}"
        );
        assert!(
            (gate["none"].as_f64().unwrap() - silent).abs() < 1e-9,
            "{v}"
        );
        assert!(gate["best"].is_null() && gate["next"].is_null(), "{v}");
    }
    // The second record is the dropped one: its gate carries the `fails` that produced the
    // "no" (0.9, above the 0.5 + 0.15 mark), and the record is absent from the kept subset.
    assert!(gates[1]["fails"].as_f64().unwrap() >= 0.65, "{v}");
    let kept: Vec<&str> = v["data"]["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["text"].as_str().unwrap())
        .collect();
    assert_eq!(kept, ["yes\n", "yes\n", "silent\n"], "{v}");
    assert_eq!(v["data"]["kept"], 3, "{v}");
    assert_eq!(v["data"]["unsure"], 1, "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn label_records_choice_gates_without_a_noul() {
    let server = common::mock_classifier(
        FakeJev {
            choose: |_, _, _| "NONE".into(),
            noul: |_, _| unreachable!(),
        }
        .with_probabilities(|_, _, labels| {
            labels
                .iter()
                .map(|l| match l.as_str() {
                    "bug" => 0.8,
                    "NONE" => 0.05,
                    _ => 0.15,
                })
                .collect()
        }),
    )
    .await;
    let mut c = common::jevify_classifier(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "label", "bug,feature"])
            .write_stdin("crash\n")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v = envelope(&out);
    let gates = decision(&v, "label", "classifier");
    assert_eq!(gates.len(), 1, "{v}");
    assert_eq!(gates[0]["best"], 0.8, "{v}");
    assert_eq!(gates[0]["next"], 0.15, "{v}");
    assert_eq!(gates[0]["none"], 0.05, "{v}");
    assert!(gates[0]["any"].is_null(), "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn why_and_pick_record_the_ranking_gate() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "ROOT"),
        noul: |_, _| 0.95,
    })
    .await;
    for (verb, args) in [
        ("why", vec!["why", "--no-save"]),
        ("pick", vec!["pick", "the root"]),
    ] {
        let mut c = common::jevify(&server);
        let out = tokio::task::spawn_blocking(move || {
            c.arg("--json")
                .args(args)
                .write_stdin("error step\nerror ROOT\nerror other\n")
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        let v = envelope(&out);
        assert_eq!(v["exit_code"], 0, "{v}");
        let gates = decision(&v, verb, "typesafe");
        assert_eq!(gates.len(), 1, "{v}");
        assert!(
            gates[0]["best"].as_f64().unwrap() > gates[0]["none"].as_f64().unwrap(),
            "{v}"
        );
        assert!(gates[0]["next"].is_number(), "{v}");
        assert_eq!(gates[0]["any"], 0.95, "{v}");
        // Round one of the one tournament: one window over the three lines, every line ranked
        // (ROOT, line 2, first), the finalists it kept, and n = 3 per window.
        let rounds = v["meta"]["decision"]["round_one"].as_array().unwrap();
        assert_eq!(rounds.len(), 1, "{v}");
        let round = &rounds[0];
        assert_eq!(round["n"], 3, "{v}");
        let windows = round["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 1, "{v}");
        let ranks = windows[0]["ranks"].as_array().unwrap();
        assert_eq!(ranks.len(), 3, "{v}");
        assert_eq!(ranks[0]["index"], 2, "{v}");
        assert!(ranks[0]["p"].as_f64().unwrap() > windows[0]["none"].as_f64().unwrap());
        assert_eq!(windows[0]["any"], 0.95, "{v}");
        let finalists = round["finalists"].as_array().unwrap();
        assert_eq!(finalists[0], 2, "{v}");
        assert!(finalists.len() <= 3, "{v}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn route_records_the_best_and_next_fit() {
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
    let inventory = dir.path().join("inv.json");
    std::fs::write(&inventory, r#"[{"name":"tar","summary":"manipulate tape archives"},{"name":"curl","summary":"transfer a URL"},{"name":"drutil","summary":"interact with CD/DVD burners"}]"#).unwrap();
    let mut c = common::jevify(&server);
    c.env("JEVIFY_INVENTORY_FILE", inventory);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "route", "burn", "a", "dvd"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v = envelope(&out);
    assert_eq!(v["data"]["tool"], "drutil", "{v}");
    let gates = decision(&v, "route", "typesafe");
    assert_eq!(gates.len(), 1, "{v}");
    assert_eq!(gates[0]["best"], v["data"]["fit"], "{v}");
    assert_eq!(gates[0]["next"], 0.2, "{v}");
    assert!(
        gates[0]["none"].is_null() && gates[0]["any"].is_null(),
        "{v}"
    );
}

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
async fn add_records_one_noul_gate_per_hunk() {
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
    let mut c = common::jevify(&server);
    c.current_dir(d.path());
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "add", "--dry-run", "the auth fix"])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v = envelope(&out);
    assert_eq!(
        decision(&v, "add", "typesafe"),
        vec![noul_gate(0.95), noul_gate(0.05)]
    );
    assert_eq!(v["data"]["hunks"].as_array().unwrap().len(), 2, "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn sort_records_one_gate_per_file() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| {
            common::option_containing(&serde_json::json!({"items": s["folders"]}), o, "Finance")
        },
        noul: |_, _| 0.9,
    })
    .await;
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("Finance")).unwrap();
    std::fs::write(d.path().join("invoice.txt"), "invoice").unwrap();
    std::fs::write(d.path().join("receipt.txt"), "receipt").unwrap();
    let out = common::jevify(&server)
        .args(["--json", "sort", d.path().to_str().unwrap()])
        .output()
        .unwrap();
    let v = envelope(&out);
    let gates = decision(&v, "sort", "typesafe");
    assert_eq!(gates.len(), 2, "{v}");
    for gate in &gates {
        assert!(gate["best"].is_number() && gate["none"].is_number(), "{v}");
        assert_eq!(gate["any"], 0.9, "{v}");
        assert!(gate["next"].is_null(), "{v}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn fill_records_one_gate_per_marker_in_argv_order() {
    let server = common::mock(FakeJev {
        choose: |_, s, o| common::option_containing(s, o, "beta"),
        noul: |i, _| if i.contains("draft") { 0.9 } else { 0.5 },
    })
    .await;
    let mut c = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args([
            "fill",
            "--dry-run",
            "--json",
            "--",
            "printf",
            "@{flag:--draft:draft}",
            "@{one:alpha|beta|gamma:which one}",
        ])
        .write_stdin("context")
        .output()
        .unwrap()
    })
    .await
    .unwrap();
    let v = envelope(&out);
    let gates = decision(&v, "fill", "typesafe");
    assert_eq!(gates.len(), 2, "{v}");
    // The flag marker is a Noul; the `one` marker is a Choice with no Noul.
    assert_eq!(gates[0], noul_gate(0.9), "{v}");
    assert!(
        gates[1]["best"].is_number() && gates[1]["none"].is_number(),
        "{v}"
    );
    assert!(gates[1]["any"].is_null(), "{v}");
    assert_eq!(v["data"]["markers"].as_array().unwrap().len(), 2, "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn answering_model_is_unknown_when_the_service_names_none() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let answers: serde_json::Map<String, Value> = body["questions"]
                .as_object()
                .unwrap()
                .keys()
                .map(|id| (id.clone(), serde_json::json!({"noul":0.9})))
                .collect();
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"answers":answers}))
        })
        .mount(&server)
        .await;
    let mut c = common::jevify(&server);
    let out = tokio::task::spawn_blocking(move || {
        c.args(["--json", "is", "holds"])
            .write_stdin("evidence")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let v = envelope(&out);
    let d = &v["meta"]["decision"];
    assert_eq!(d["model"]["requested"], "jev-1.13.0", "{v}");
    assert_eq!(d["model"]["answering"], "unknown", "{v}");
    assert_eq!(d["gates"], serde_json::json!([noul_gate(0.9)]), "{v}");
}

#[test]
fn a_usage_error_carries_the_verb_and_an_unknown_model() {
    let out = common::bin().args(["--json", "is"]).output().unwrap();
    let v = envelope(&out);
    assert_eq!(v["exit_code"], 2, "{v}");
    let d = &v["meta"]["decision"];
    assert_eq!(d["verb"], "is", "{v}");
    assert_eq!(d["model"]["answering"], "unknown", "{v}");
    assert!(d["model"]["requested"].is_null(), "{v}");
    assert_eq!(d["gates"], serde_json::json!([]), "{v}");
}
