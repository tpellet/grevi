#![allow(dead_code)]
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Decides answers from the request. `choose` gets (instructions, state, options) and returns the option key.
/// `noul` gets (instructions, state) and returns P(yes).
#[derive(Clone)]
pub struct FakeJev {
    pub choose: fn(&str, &Value, &[String]) -> String,
    pub noul: fn(&str, &Value) -> f64,
}

impl Respond for FakeJev {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let state = &body["state"];
        let mut answers = serde_json::Map::new();
        for (id, q) in body["questions"].as_object().unwrap() {
            let instr = q["instructions"].as_str().unwrap_or_default();
            if q["type"] == "noul" {
                answers.insert(id.clone(), json!({ "noul": (self.noul)(instr, state) }));
            } else {
                let opts: Vec<String> =
                    q["criteria"].as_object().unwrap().keys().cloned().collect();
                let pick = (self.choose)(instr, state, &opts);
                let rest = 0.1 / (opts.len().max(2) - 1) as f64;
                let probs: serde_json::Map<String, Value> = opts
                    .iter()
                    .map(|o| (o.clone(), json!(if *o == pick { 0.9 } else { rest })))
                    .collect();
                answers.insert(
                    id.clone(),
                    json!({ "choice": pick, "probabilities": probs, "confidence": 0.8 }),
                );
            }
        }
        ResponseTemplate::new(200).set_body_json(json!({
            "model": "jev-fake", "answers": answers, "usage": { "input_tokens": 100, "output_tokens": 10 }
        }))
    }
}

pub async fn mock(fake: FakeJev) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(fake)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models":[]})))
        .mount(&server)
        .await;
    server
}

/// The binary with every hunch variable removed. A developer's shell may export them
/// (`HUNCH_THRESHOLD=2` turns even `capabilities` into exit 2, since `Config::load` runs for
/// every verb); the contract tests must not depend on it. Every raw binary invocation from
/// Task 3 onward starts here; `cli_basics.rs` (Task 1, env-immune by construction) and
/// `live.rs` (needs the inherited key) are the two raw `cargo_bin` exceptions.
pub fn bin() -> assert_cmd::Command {
    let mut c = assert_cmd::Command::cargo_bin("hunch").unwrap();
    for var in [
        "TYPESAFE_API_KEY",
        "TYPESAFE_API_KEY_FILE",
        "HUNCH_BASE_URL",
        "HUNCH_THRESHOLD",
        "HUNCH_MODEL",
        "HUNCH_CONCURRENCY",
        "HUNCH_CACHE_DIR",
        "HUNCH_NO_CACHE",
        "HUNCH_PRICE_PER_MTOK",
        "HUNCH_NO_PREWARM",
        "HUNCH_INVENTORY_FILE",
    ] {
        c.env_remove(var);
    }
    c
}

/// The binary, pointed at the mock server, no cache, no key file.
pub fn hunch(server: &MockServer) -> assert_cmd::Command {
    let mut c = bin();
    c.env("HUNCH_BASE_URL", server.uri())
        .env("TYPESAFE_API_KEY", "test-key")
        .env("HUNCH_NO_CACHE", "1");
    c
}

/// A library `Config` for in-process tests, built literally so no test touches the process env.
pub fn config(server: &MockServer) -> hunch::config::Config {
    hunch::config::Config {
        key: Some("test-key".into()),
        key_file: None,
        base_url: server.uri(),
        model: "jev-1.13.0".into(),
        threshold: 0.5,
        concurrency: 8,
        cache_dir: None,
        price_per_mtok: 0.042,
        prewarm: false,
        stats: std::sync::Arc::new(hunch::jev::client::Stats::default()),
    }
}

/// The option whose line text (from state.items) contains `needle`, else NONE.
pub fn option_containing(state: &Value, opts: &[String], needle: &str) -> String {
    let items = state["items"].as_array().cloned().unwrap_or_default();
    for it in items {
        let s = it.as_str().unwrap_or_default();
        if s.contains(needle) {
            if let Some(id) = s.strip_prefix('[').and_then(|r| r.split(']').next()) {
                if opts.iter().any(|o| o == id) {
                    return id.to_string();
                }
            }
        }
    }
    "NONE".into()
}
