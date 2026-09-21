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
