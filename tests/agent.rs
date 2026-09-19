mod common;

#[test]
fn capabilities_lists_verbs_exit_codes_env() {
    let out = common::bin()
        .args(["capabilities", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = &v["data"];
    for verb in ["pick", "why", "run", "is"] {
        assert!(
            d["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == verb),
            "{verb}"
        );
    }
    // Wave 2: both `add` (Task 14) and `sort` (Task 15) are listed.
    for verb in ["add", "sort"] {
        assert!(
            d["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == verb),
            "{verb}"
        );
    }
    assert_eq!(d["exit_codes"].as_array().unwrap().len(), 9);
    assert!(
        d["env"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "TYPESAFE_API_KEY")
    );
    assert_eq!(d["limits"]["choice_options"], 255);
    assert_eq!(
        d["env"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == "HUNCH_MODEL")
            .unwrap()["default"],
        "jev-1.13.0"
    );
}

#[test]
fn robot_docs_topics() {
    for t in ["guide", "commands", "exit-codes", "examples", "privacy"] {
        common::bin().args(["robot-docs", t]).assert().success();
    }
    common::bin().args(["robot-docs", "nope"]).assert().code(2);
}

#[test]
fn health_without_key_is_auth_error() {
    common::bin().args(["health", "--json"]).assert().code(5);
}

// A bad key file must not break verbs that need no key.
#[test]
fn capabilities_work_with_an_unreadable_key_file() {
    common::bin()
        .env("TYPESAFE_API_KEY_FILE", "/nonexistent/hunch-key")
        .args(["capabilities", "--json"])
        .assert()
        .success();
}
