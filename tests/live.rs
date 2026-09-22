//! Live tests: `TYPESAFE_API_KEY_FILE=$HOME/.ssh/typesafe-ai-key cargo test --test live -- --ignored --test-threads=1`
//! (sandbox disabled). Without a key they print SKIPPED and pass; the hand-off must list them as NOT RUN.

pub fn live_key_present() -> bool {
    if std::env::var_os("TYPESAFE_API_KEY").is_some()
        || std::env::var_os("TYPESAFE_API_KEY_FILE").is_some()
    {
        return true;
    }
    eprintln!(
        "SKIPPED: set TYPESAFE_API_KEY_FILE=$HOME/.ssh/typesafe-ai-key (or TYPESAFE_API_KEY) to run live tests"
    );
    false
}

/// A repository with a known set of branches, so the branch case does not depend on the
/// branches a developer happens to carry in this checkout.
fn branch_fixture() -> std::path::PathBuf {
    let root = tempfile::tempdir().unwrap().keep();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .current_dir(&root)
            .args([
                "-c",
                "user.name=example",
                "-c",
                "user.email=example@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    std::fs::write(root.join("README"), "fixture\n").unwrap();
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "initial"]);
    git(&["branch", "auth-refactor"]);
    git(&["branch", "payment-timeout"]);
    root
}

#[test]
#[ignore]
fn live_fill_branch_and_one_per_backend() {
    let repo = branch_fixture();
    for backend in ["classifier", "typesafe"] {
        if backend == "typesafe" && !live_key_present() {
            continue;
        }
        for (marker, context) in [
            ("@{branch:the main development branch}", ""),
            (
                "@{one:bug|feature|docs:what kind of report is this}",
                "The program crashes on empty input; this is a reproducible defect.",
            ),
        ] {
            let mut command = assert_cmd::Command::cargo_bin("jevify").unwrap();
            command
                .current_dir(&repo)
                .env("JEVIFY_BACKEND", backend)
                .env("JEVIFY_NO_CACHE", "1")
                .env("JEVIFY_CACHE_DIR", tempfile::tempdir().unwrap().keep());
            if backend == "classifier" {
                command
                    .env_remove("TYPESAFE_API_KEY")
                    .env_remove("TYPESAFE_API_KEY_FILE");
            }
            let out = command
                .args(["fill", "--dry-run", "--json", "--", "printf", marker])
                .write_stdin(context)
                .output()
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(out.status.code(), Some(0), "{value}");
            assert_eq!(
                value["data"]["argv"][1],
                if marker.starts_with("@{branch:") {
                    "main"
                } else {
                    "bug"
                }
            );
            assert!(jevify::jev::all_jev(
                value["meta"]["model"].as_str().unwrap()
            ));
            assert_eq!(value["meta"]["backend"], backend);
        }
    }
}

#[test]
#[ignore]
fn live_filter_one_batch_per_backend() {
    for backend in ["classifier", "typesafe"] {
        if backend == "typesafe" && !live_key_present() {
            continue;
        }
        let mut cmd = assert_cmd::Command::cargo_bin("jevify").unwrap();
        cmd.env("JEVIFY_BACKEND", backend)
            .env("JEVIFY_NO_CACHE", "1")
            .env("JEVIFY_CACHE_DIR", tempfile::tempdir().unwrap().keep());
        if backend == "classifier" {
            cmd.env_remove("TYPESAFE_API_KEY")
                .env_remove("TYPESAFE_API_KEY_FILE");
        }
        let out = cmd
            .args([
                "filter",
                "reports a failed assertion",
                "--json",
                "--no-save",
            ])
            .write_stdin("assertion failed: left == right\ntest result: ok. 12 passed\n")
            .output()
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(0), "{value}");
        assert_eq!(
            value["data"]["records"][0]["text"],
            "assertion failed: left == right\n"
        );
        assert_eq!(value["meta"]["backend"], backend);
        assert_eq!(value["meta"]["requests"], 1);
        assert!(value["meta"]["model"].is_string());
    }
}

// Needs no key: `man` and PATH only. Records the machine's inventory size and cache speed.
#[test]
#[ignore]
fn inventory_finds_many_tools_here() {
    let cache = tempfile::tempdir().unwrap();
    let t0 = std::time::Instant::now();
    let tools = jevify::inventory::load(Some(cache.path())).unwrap();
    let cold = t0.elapsed();
    let t1 = std::time::Instant::now();
    let again = jevify::inventory::load(Some(cache.path())).unwrap();
    let warm = t1.elapsed();
    eprintln!(
        "{} tools; cold {cold:?}, cached {warm:?} (informational, not asserted)",
        tools.len()
    );
    assert_eq!(tools.len(), again.len());
    let documented = tools
        .iter()
        .filter(|t| t.summary != "(no man page)")
        .count();
    assert!(
        documented > 300,
        "{documented} documented tools: `man -k` gave nothing (run outside the sandbox)"
    );
    assert!(tools.iter().any(|t| t.name == "tar"));
    for modern in ["rg", "fd", "jq", "uv"] {
        if std::process::Command::new("which")
            .arg(modern)
            .output()
            .is_ok_and(|o| o.status.success())
        {
            assert!(
                tools.iter().any(|t| t.name == modern),
                "{modern} is installed but missing from the inventory"
            );
        }
    }
}

/// Needs no key: classifier.dev is free and keyless, which is the whole point of the backend.
/// Also the parity check in one assertion — the model that answers there is Jev.
#[test]
#[ignore]
fn live_classifier_picks_without_a_key() {
    let mut cmd = assert_cmd::Command::cargo_bin("jevify").unwrap();
    cmd.env_remove("TYPESAFE_API_KEY")
        .env_remove("TYPESAFE_API_KEY_FILE")
        .env("JEVIFY_BACKEND", "classifier")
        .env("JEVIFY_NO_CACHE", "1");
    let out = cmd
        .args(["--json", "pick", "the invoice from March"])
        .write_stdin("notes.txt\ninvoice-2026-03.pdf\ncat.jpg\n")
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(
        v["data"]["matches"][0]["text"], "invoice-2026-03.pdf",
        "{v}"
    );
    assert_eq!(v["meta"]["backend"], "classifier");
    let model = v["meta"]["model"].as_str().unwrap_or_default();
    assert!(
        model.starts_with("jev"),
        "classifier.dev answered with `{model}`"
    );
    // Free, and `meta` says so rather than pricing tokens nobody was charged for.
    assert_eq!(v["meta"]["cost_usd"], 0.0, "{v}");
}

#[test]
#[ignore]
fn live_classifier_health_is_reachable() {
    let mut cmd = assert_cmd::Command::cargo_bin("jevify").unwrap();
    cmd.env_remove("TYPESAFE_API_KEY")
        .env_remove("TYPESAFE_API_KEY_FILE")
        .env("JEVIFY_BACKEND", "classifier");
    let out = cmd.args(["--json", "health"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["data"]["backend"], "classifier");
    assert_eq!(v["data"]["key"], "not needed");
}

#[test]
#[ignore]
fn live_route_routes_tar() {
    if !live_key_present() {
        return;
    }
    let out = assert_cmd::Command::cargo_bin("jevify")
        .unwrap()
        .env("JEVIFY_BACKEND", "typesafe")
        .args(["--json", "route", "extract", "the", "gzipped", "archive"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        ["tar", "bsdtar", "gunzip", "gzip"].contains(&v["data"]["tool"].as_str().unwrap_or("")),
        "{v}"
    );
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["meta"]["backend"], "typesafe");
    assert!(
        v["meta"]["model"]
            .as_str()
            .unwrap_or_default()
            .starts_with("jev")
    );
}

// ---- fill, label and filter on both backends (hunch-3te) -------------------------------------
//
// The cases are the held-out sets of the 0.8.x evaluations: one marker per kind over this
// repository's own history and files, `evals/live/titles.tsv` (30 issue titles with their gold
// label and a crash-or-hang flag) and `evals/live/tests.txt` (70 test names). A test passes on
// the gold answer or an honest abstention (exit 3 with a reason), never on a wrong pick.

const TITLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/evals/live/titles.tsv");
const TESTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/evals/live/tests.txt");

/// A `jevify` process on `backend`, or `None` (with SKIPPED on stderr) when TypeSafe has no key.
fn live_command(backend: &str) -> Option<assert_cmd::Command> {
    if backend == "typesafe" && !live_key_present() {
        return None;
    }
    let mut cmd = assert_cmd::Command::cargo_bin("jevify").unwrap();
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("JEVIFY_BACKEND", backend)
        .env("JEVIFY_NO_CACHE", "1")
        .env("JEVIFY_CACHE_DIR", tempfile::tempdir().unwrap().keep());
    if backend == "classifier" {
        cmd.env_remove("TYPESAFE_API_KEY")
            .env_remove("TYPESAFE_API_KEY_FILE");
    }
    Some(cmd)
}

/// The envelope shape every verb shares. Returns the parsed envelope.
fn live_envelope(out: &std::process::Output, command: &str, backend: &str) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{command} on {backend}: no JSON envelope ({e}); stdout {:?}; stderr {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert_eq!(value["command"], command, "{value}");
    assert_eq!(value["exit_code"], out.status.code().unwrap(), "{value}");
    // `ok` means no error: an abstention is ok with exit 3.
    assert_eq!(value["ok"], value["error"].is_null(), "{value}");
    assert!(value["version"].is_string(), "{value}");
    assert_eq!(value["meta"]["backend"], backend, "{value}");
    assert!(value["meta"]["model"].is_string(), "{value}");
    value
}

/// classifier.dev sometimes answers with another model: that is SKIPPED, not a verdict on jevify.
/// With a key, TypeSafe must answer with Jev.
fn answered_by_jev(value: &serde_json::Value, backend: &str) -> bool {
    let model = value["meta"]["model"].as_str().unwrap_or_default();
    if jevify::jev::all_jev(model) {
        return true;
    }
    assert_eq!(
        backend, "classifier",
        "typesafe answered with `{model}`, not Jev: {value}"
    );
    eprintln!("SKIPPED: classifier.dev answered with `{model}`, not Jev");
    false
}

/// A fill case: the argv after `--`, the stdin, the accepted handles for the marker. The
/// branch case reads the clock, not a name, so it holds on a fresh clone as well as here.
struct FillCase {
    kind: &'static str,
    argv: &'static [&'static str],
    stdin: String,
    gold: &'static [&'static str],
}

fn fill_cases() -> Vec<FillCase> {
    let crash = "Segmentation fault after upgrading to macOS 26\n".to_owned();
    vec![
        FillCase {
            kind: "-",
            argv: &["cargo", "test", "@{-:an HTTP 422 counts as an input error}"],
            stdin: std::fs::read_to_string(TESTS).unwrap(),
            gold: &["http_422_is_an_input_error_not_an_outage"],
        },
        FillCase {
            kind: "branch",
            argv: &[
                "git",
                "log",
                "-1",
                "@{branch:the most recently updated branch}",
            ],
            stdin: String::new(),
            gold: &["main", "origin/main"],
        },
        FillCase {
            kind: "commit",
            argv: &[
                "git",
                "show",
                "@{commit:renamed the project from hunch to grevi}",
            ],
            stdin: String::new(),
            gold: &["441703a6dcd133eb06563484d5cd0d486222605a"],
        },
        FillCase {
            kind: "file",
            argv: &["wc", "-l", "@{file:the marker parser}"],
            stdin: String::new(),
            gold: &["src/marker.rs"],
        },
        FillCase {
            kind: "dir",
            argv: &["ls", "@{dir:the GitHub Actions workflows}"],
            stdin: String::new(),
            gold: &[".github/workflows"],
        },
        FillCase {
            kind: "tool",
            argv: &["env", "@{tool:the Rust package manager}", "--version"],
            stdin: String::new(),
            gold: &["cargo"],
        },
        FillCase {
            kind: "ci-run",
            argv: &[
                "gh",
                "run",
                "view",
                "@{ci-run:the failed ci run on the v0.7.0 tag}",
            ],
            stdin: String::new(),
            gold: &["35736209634"],
        },
        FillCase {
            kind: "one",
            argv: &[
                "echo",
                "--label=@{one:bug|feature|docs:what kind of report is this}",
            ],
            stdin: crash.clone(),
            gold: &["bug"],
        },
        FillCase {
            kind: "flag",
            argv: &[
                "echo",
                "@{flag:--urgent:the report describes a crash or data loss}",
            ],
            stdin: crash,
            gold: &["--urgent"],
        },
    ]
}

/// `gh` listing runs needs the CLI and a login; without them the `ci-run` case is SKIPPED.
fn gh_available() -> bool {
    let ok = std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("SKIPPED: ci-run needs `gh` logged in to GitHub");
    }
    ok
}

fn live_fill_kinds(backend: &str) {
    for case in fill_cases() {
        if case.kind == "ci-run" && !gh_available() {
            continue;
        }
        let Some(mut cmd) = live_command(backend) else {
            return;
        };
        let out = cmd
            .args(["fill", "--dry-run", "--json", "--"])
            .args(case.argv)
            .write_stdin(case.stdin)
            .output()
            .unwrap();
        let value = live_envelope(&out, "fill", backend);
        if !answered_by_jev(&value, backend) {
            continue;
        }
        let marker = &value["data"]["markers"][0];
        assert_eq!(marker["kind"], case.kind, "{value}");
        // A flag is one yes-or-no question over the context: it lists no candidates.
        let least = if case.kind == "flag" { 0 } else { 1 };
        assert!(marker["candidates"].as_u64().unwrap() >= least, "{value}");
        match out.status.code() {
            Some(0) => {
                let handle = marker["handle"].as_str().unwrap_or_default();
                let argv = value["data"]["argv"].as_array().unwrap();
                assert!(
                    case.gold.contains(&handle),
                    "{backend} {}: picked `{handle}`, gold {:?}: {value}",
                    case.kind,
                    case.gold
                );
                assert!(marker["p"].as_f64().unwrap() > 0.0, "{value}");
                assert!(
                    argv.iter().any(|a| a.as_str().unwrap().contains(handle)),
                    "resolved argv lacks the handle: {value}"
                );
                assert_eq!(argv[0], case.argv[0], "{value}");
                eprintln!("{backend} {}: {handle} p {}", case.kind, marker["p"]);
            }
            Some(3) => {
                assert!(value["data"]["reason"].is_string(), "{value}");
                assert!(marker["handle"].is_null(), "{value}");
                assert!(value["data"].get("argv").is_none(), "{value}");
                eprintln!(
                    "{backend} {}: abstained ({}), gold {:?}",
                    case.kind, value["data"]["reason"], case.gold
                );
            }
            code => panic!("{backend} {}: exit {code:?}: {value}", case.kind),
        }
    }
}

#[test]
#[ignore]
fn live_fill_every_kind_on_typesafe() {
    live_fill_kinds("typesafe");
}

#[test]
#[ignore]
fn live_fill_every_kind_on_classifier() {
    live_fill_kinds("classifier");
}

/// (gold label, crash-or-hang, title) per line of `evals/live/titles.tsv`.
fn titles() -> Vec<(String, bool, String)> {
    std::fs::read_to_string(TITLES)
        .unwrap()
        .lines()
        .map(|line| {
            let mut fields = line.splitn(3, '\t');
            let label = fields.next().unwrap().to_owned();
            let crash = fields.next().unwrap() == "1";
            (label, crash, fields.next().unwrap().to_owned())
        })
        .collect()
}

fn live_label_thirty(backend: &str) {
    let rows = titles();
    assert_eq!(rows.len(), 30);
    let stdin: String = rows.iter().map(|(_, _, t)| format!("{t}\n")).collect();
    let Some(mut cmd) = live_command(backend) else {
        return;
    };
    let out = cmd
        .args(["label", "bug,feature,docs,question", "--json"])
        .write_stdin(stdin)
        .output()
        .unwrap();
    let value = live_envelope(&out, "label", backend);
    if !answered_by_jev(&value, backend) {
        return;
    }
    assert_eq!(out.status.code(), Some(0), "{value}");
    assert_eq!(value["data"]["total"], 30, "{value}");
    assert_eq!(value["data"]["complete"], true, "{value}");
    let records = value["data"]["records"].as_array().unwrap();
    assert_eq!(records.len(), 30, "{value}");
    // The held-out measurement is 29 of 30 with one unsure; `Pod kind should read the current
    // namespace` reads as a bug or a feature, so one label off the gold is within the set.
    let mut unsure = 0;
    let mut wrong = Vec::new();
    for (record, (gold, _, title)) in records.iter().zip(&rows) {
        assert_eq!(record["text"], format!("{title}\n"), "{value}");
        let label = record["label"].as_str().unwrap();
        if label == "?" {
            unsure += 1;
        } else if label != gold {
            wrong.push(format!("`{title}` got {label}, gold {gold}"));
        }
    }
    assert_eq!(value["data"]["unsure"], unsure, "{value}");
    assert_eq!(value["data"]["labelled"], 30 - unsure, "{value}");
    assert!(wrong.len() <= 1, "{wrong:?}: {value}");
    assert!(unsure <= 6, "{unsure} of 30 unsure: {value}");
    eprintln!(
        "{backend} label: {} of 30 right, {unsure} unsure, wrong {wrong:?}",
        30 - unsure - wrong.len()
    );
}

#[test]
#[ignore]
fn live_label_thirty_titles_on_typesafe() {
    live_label_thirty("typesafe");
}

#[test]
#[ignore]
fn live_label_thirty_titles_on_classifier() {
    live_label_thirty("classifier");
}

fn live_filter_crashes(backend: &str) {
    let rows = titles();
    let stdin: String = rows.iter().map(|(_, _, t)| format!("{t}\n")).collect();
    let positives: Vec<&str> = rows
        .iter()
        .filter(|(_, crash, _)| *crash)
        .map(|(_, _, t)| t.as_str())
        .collect();
    assert_eq!(positives.len(), 7);
    let Some(mut cmd) = live_command(backend) else {
        return;
    };
    let out = cmd
        .args(["filter", "reports a crash or a hang", "--json", "--no-save"])
        .write_stdin(stdin)
        .output()
        .unwrap();
    let value = live_envelope(&out, "filter", backend);
    if !answered_by_jev(&value, backend) {
        return;
    }
    assert_eq!(out.status.code(), Some(0), "{value}");
    assert_eq!(value["data"]["total"], 30, "{value}");
    // --no-save: nothing saved, and `complete` says so.
    assert_eq!(value["data"]["complete"], false, "{value}");
    assert!(value["data"]["saved_input"].is_null(), "{value}");
    let records = value["data"]["records"].as_array().unwrap();
    assert_eq!(value["data"]["kept"], records.len(), "{value}");
    let mut unsure = 0;
    for record in records {
        let title = record["text"].as_str().unwrap().trim_end_matches('\n');
        match record["verdict"].as_str().unwrap() {
            "yes" => assert!(
                positives.contains(&title),
                "`{title}` kept as yes, not a crash or a hang: {value}"
            ),
            "unsure" => unsure += 1,
            other => panic!("kept record with verdict {other}: {value}"),
        }
    }
    assert_eq!(value["data"]["unsure"], unsure, "{value}");
    for title in &positives {
        assert!(
            records
                .iter()
                .any(|r| r["text"].as_str().unwrap().trim_end_matches('\n') == *title),
            "`{title}` lost: {value}"
        );
    }
    eprintln!(
        "{backend} filter: kept {} of 30, {unsure} unsure, all 7 crashes kept",
        records.len()
    );
}

#[test]
#[ignore]
fn live_filter_crash_titles_on_typesafe() {
    live_filter_crashes("typesafe");
}

#[test]
#[ignore]
fn live_filter_crash_titles_on_classifier() {
    live_filter_crashes("classifier");
}
