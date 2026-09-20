//! Exercise publication boundaries without signing tags or contacting services.
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

const SHA: &str = "1234567890abcdef1234567890abcdef12345678";

fn run(case: &str, args: &[&str]) -> (std::process::Output, String) {
    // Keep scratch evidence; repository policy forbids deleting even temporary files.
    let root = tempfile::Builder::new()
        .prefix("grevi-release-test-")
        .tempdir()
        .unwrap()
        .keep();
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    fs::write(root.join("Cargo.toml"), "version = \"99.99.99\"\n").unwrap();
    let stub = r##"#!/usr/bin/env bash
set -eu
name=${0##*/}
printf '%s %s\n' "$name" "$*" >> "$RELEASE_TEST_LOG"
case "$name" in
  git)
    case "$1" in
      rev-parse) printf '%s\n' "$RELEASE_TEST_ROOT" ;;
      fetch) [[ $RELEASE_TEST_CASE != fetch_error ]] ;;
      merge-base) [[ $RELEASE_TEST_CASE != foreign ]] ;;
      show)
        if [[ $RELEASE_TEST_CASE == bad_version ]]; then
          printf '[package]\nversion = "0.3.4-rc.1"\n'
        else
          printf '[package]\nversion = "0.3.4"\n[dependencies]\nversion = "9.9.9"\n'
        fi ;;
      show-ref) [[ $RELEASE_TEST_CASE == tag_exists ]] ;;
      describe) printf 'v0.3.3\n' ;;
      tag|push) : ;;
      *) exit 99 ;;
    esac ;;
  gh)
    if [[ $1 == repo ]]; then
      if [[ $RELEASE_TEST_CASE == bad_repo ]]; then printf 'someone/else\n'; else printf 'tpellet/grevi\n'; fi
    else
      case "$RELEASE_TEST_CASE" in
        missing|cancelled|failure|pending) printf '%s\n' "$RELEASE_TEST_CASE" ;;
        *) printf 'success\n' ;;
      esac
    fi ;;
  curl)
    case "$RELEASE_TEST_CASE" in
      registry_exists) printf '200' ;;
      registry_error) printf '503' ;;
      network_error) exit 7 ;;
      *) printf '404' ;;
    esac ;;
  gitleaks) [[ $RELEASE_TEST_CASE != scan_error ]] ;;
  *) exit 99 ;;
esac
"##;
    for name in ["git", "gh", "curl", "gitleaks"] {
        let path = bin.join(name);
        fs::write(&path, stub).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let log = root.join("commands.log");
    let output = Command::new("bash")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/release.sh"))
        .args(args)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("RELEASE_TEST_ROOT", &root)
        .env("RELEASE_TEST_LOG", &log)
        .env("RELEASE_TEST_CASE", case)
        .output()
        .unwrap();
    (output, fs::read_to_string(log).unwrap_or_default())
}

#[test]
fn release_signs_only_the_green_committed_version() {
    let (out, log) = run("success", &[SHA]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(log.contains(&format!("git show {SHA}:Cargo.toml")));
    assert!(log.contains(&format!("--branch main --event push --commit {SHA}")));
    assert!(log.contains(&format!("gitleaks git . --log-opts=v0.3.3..{SHA} --redact")));
    assert!(log.contains(&format!("git tag -s v0.3.4 {SHA} -m Release v0.3.4")));
    assert!(log.ends_with("git push origin refs/tags/v0.3.4\n"));
    assert!(!log.contains("99.99.99"));
}

#[test]
fn release_refuses_unverified_or_existing_versions_before_tagging() {
    for case in [
        "missing",
        "cancelled",
        "failure",
        "pending",
        "foreign",
        "bad_repo",
        "bad_version",
        "tag_exists",
        "registry_exists",
        "registry_error",
        "network_error",
        "fetch_error",
        "scan_error",
    ] {
        let (out, log) = run(case, &[SHA]);
        assert!(!out.status.success(), "accepted {case}: {log}");
        assert!(!log.contains("git tag "), "tagged on {case}: {log}");
        assert!(!log.contains("git push "), "pushed on {case}: {log}");
    }
}

#[test]
fn release_requires_one_full_sha_before_any_external_call() {
    for args in [
        vec![],
        vec![""],
        vec!["HEAD"],
        vec!["--help"],
        vec![SHA, SHA],
    ] {
        let (out, log) = run("success", &args);
        assert!(!out.status.success());
        assert!(log.is_empty(), "unexpected calls: {log}");
    }
}
