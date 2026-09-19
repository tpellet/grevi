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
    let tools = hunch::inventory::load(Some(cache.path())).unwrap();
    let cold = t0.elapsed();
    let t1 = std::time::Instant::now();
    let again = hunch::inventory::load(Some(cache.path())).unwrap();
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

#[test]
#[ignore]
fn live_run_routes_tar() {
    if !live_key_present() {
        return;
    }
    let out = assert_cmd::Command::cargo_bin("hunch")
        .unwrap()
        .args([
            "--json",
            "run",
            "--dry-run",
            "extract",
            "the",
            "gzipped",
            "archive",
        ])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        ["tar", "bsdtar", "gunzip", "gzip"].contains(&v["data"]["tool"].as_str().unwrap_or("")),
        "{v}"
    );
}
