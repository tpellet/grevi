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

/// The same answers over classifier.dev's wire format: one item, one dimension per question,
/// `scores` per label. Built from a `FakeJev` so a test can point either backend at the same
/// expectations and compare.
#[derive(Clone)]
pub struct FakeClassifier(pub FakeJev);

impl Respond for FakeClassifier {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let text = body["items"][0].as_str().unwrap_or_default();
        assert!(!text.is_empty() && text.encode_utf16().count() <= 32_000);
        assert!(body["dimensions"].to_string().encode_utf16().count() <= 16_000);
        // grevi serializes a non-string state as compact JSON and sends a string state as
        // itself; this reverses that so the shared `choose`/`noul` closures see the state.
        let state = match serde_json::from_str::<Value>(text) {
            Ok(v) if v.is_object() || v.is_array() => v,
            _ => Value::String(text.to_string()),
        };
        let mut dims = serde_json::Map::new();
        for (id, d) in body["dimensions"].as_object().unwrap() {
            assert!(!id.trim().is_empty() && id.encode_utf16().count() <= 64);
            let instr = d["instructions"].as_str().unwrap_or_default();
            assert!(instr.encode_utf16().count() <= 4_000);
            let labels: Vec<String> = d["labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap().to_string())
                .collect();
            assert!(
                labels
                    .iter()
                    .all(|label| !label.trim().is_empty() && label.encode_utf16().count() <= 200)
            );
            assert!(
                labels.len() >= 2 && labels.len() <= 100,
                "classifier.dev takes 2..=100 labels, got {}",
                labels.len()
            );
            // A Noul arrives as a two-label dimension whose first label is the `true` side;
            // every grevi Choice carries NONE, so that tells the two apart.
            let is_noul = labels.len() == 2 && !labels.iter().any(|l| l == "NONE");
            let (pick, top) = if is_noul {
                let p = (self.0.noul)(instr, &state);
                (labels[usize::from(p < 0.5)].clone(), p.max(1.0 - p))
            } else {
                ((self.0.choose)(instr, &state, &labels), 0.9)
            };
            let rest = (1.0 - top) / (labels.len().max(2) - 1) as f64;
            let scores: serde_json::Map<String, Value> = labels
                .iter()
                .map(|l| {
                    (
                        l.clone(),
                        json!(if *l == pick {
                            top
                        } else if is_noul {
                            1.0 - top
                        } else {
                            rest
                        }),
                    )
                })
                .collect();
            dims.insert(
                id.clone(),
                json!({ "label": pick, "confidence": top, "scores": scores, "model": "jev-fake", "ms": 1 }),
            );
        }
        let n = dims.len();
        assert!(
            n <= 20,
            "classifier.dev takes at most 20 dimensions, got {n}"
        );
        ResponseTemplate::new(200).set_body_json(json!({
            "model": "jev-fake",
            "results": [{ "dimensions": dims }],
            "usage": { "items": 1, "dimensions": n, "classifications": n }
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

/// A mock classifier.dev: the classify endpoint and the health probe.
pub async fn mock_classifier(fake: FakeJev) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/classify"))
        .respond_with(FakeClassifier(fake))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    server
}

/// The binary with every grevi variable removed. A developer's shell may export them
/// (`GREVI_THRESHOLD=2` turns even `capabilities` into exit 2, since `Config::load` runs for
/// every verb); the contract tests must not depend on it. Every raw binary invocation from
/// Task 3 onward starts here; `cli_basics.rs` (Task 1, env-immune by construction) and
/// `live.rs` (needs the inherited key) are the two raw `cargo_bin` exceptions.
pub fn bin() -> assert_cmd::Command {
    let mut c = assert_cmd::Command::cargo_bin("grevi").unwrap();
    for var in [
        "TYPESAFE_API_KEY",
        "TYPESAFE_API_KEY_FILE",
        "GREVI_BACKEND",
        "GREVI_BASE_URL",
        "GREVI_THRESHOLD",
        "GREVI_MODEL",
        "GREVI_CONCURRENCY",
        "GREVI_CACHE_DIR",
        "GREVI_NO_CACHE",
        "GREVI_PRICE_PER_MTOK",
        "GREVI_INVENTORY_FILE",
    ] {
        c.env_remove(var);
    }
    c
}

/// The binary, pointed at the mock server, no cache, no key file.
pub fn grevi(server: &MockServer) -> assert_cmd::Command {
    let mut c = bin();
    c.env("GREVI_BASE_URL", server.uri())
        .env("TYPESAFE_API_KEY", "test-key")
        .env("GREVI_NO_CACHE", "1");
    c
}

/// The binary, pointed at a mock classifier.dev, with no key at all.
pub fn grevi_classifier(server: &MockServer) -> assert_cmd::Command {
    let mut c = bin();
    c.env("GREVI_BACKEND", "classifier")
        .env("GREVI_BASE_URL", server.uri())
        .env("GREVI_NO_CACHE", "1");
    c
}

/// A library `Config` for in-process tests, built literally so no test touches the process env.
pub fn config(server: &MockServer) -> grevi::config::Config {
    grevi::config::Config {
        backend: grevi::config::Backend::Typesafe,
        key: Some("test-key".into()),
        key_file: None,
        base_url: server.uri(),
        model: "jev-1.13.0".into(),
        threshold: 0.5,
        concurrency: 8,
        cache_dir: None,
        price_per_mtok: 0.042,
        stats: std::sync::Arc::new(grevi::jev::client::Stats::default()),
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
