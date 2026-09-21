use serde::Serialize;

pub fn shell_quote(argv: &[std::ffi::OsString]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let mut output = Vec::new();
    for (index, arg) in argv.iter().enumerate() {
        if index > 0 {
            output.push(b' ');
        }
        output.push(b'\'');
        for &byte in arg.as_bytes() {
            if byte == b'\'' {
                output.extend_from_slice(b"'\\''");
            } else {
                output.push(byte);
            }
        }
        output.push(b'\'');
    }
    output
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct AttemptCounts {
    pub attempted: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub in_flight: u64,
}

#[derive(Serialize, Debug, Clone)]
pub struct TokenAccounting {
    pub reported_subtotal: u64,
    pub reported_attempts: u64,
    pub unknown_attempts: u64,
    pub complete: bool,
}

impl Default for TokenAccounting {
    fn default() -> Self {
        Self {
            reported_subtotal: 0,
            reported_attempts: 0,
            unknown_attempts: 0,
            complete: true,
        }
    }
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct UsageAccounting {
    pub input_tokens: TokenAccounting,
    pub output_tokens: TokenAccounting,
}

#[derive(Serialize, Debug, Clone)]
pub struct CostEstimate {
    pub basis: &'static str,
    pub input_price_per_mtok: f64,
    pub reported_input_subtotal_usd: f64,
    pub complete: bool,
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct Telemetry {
    pub inference_posts: AttemptCounts,
    pub health_gets: AttemptCounts,
    pub prewarm_gets: AttemptCounts,
    pub semantic_calls: AttemptCounts,
    pub semantic_questions: u64,
    pub retry_sends: u64,
    pub retry_sleep_ms: u64,
    pub usage: UsageAccounting,
    /// The client cannot infer logical rounds from physical requests.
    pub logical_rounds: Option<u64>,
    pub cost_estimate: Option<CostEstimate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Human,
    Json,
    Jsonl,
    Toon,
}

/// `Default` is the meta of a command that never reached a backend (a usage error before
/// `Config` loaded): `backend` is then the empty string, since nothing answered.
#[derive(Serialize, Default, Debug, Clone)]
pub struct Meta {
    /// Which API answered: `typesafe` or `classifier`. Both run Jev; `model` says which build.
    pub backend: &'static str,
    pub model: Option<String>,
    pub elapsed_ms: u128,
    pub requests: u64,
    pub cache_hits: u32,
    pub input_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub threshold: f64,
    /// `x-typesafe-request-id` of the last TypeSafe response seen (success or failure); what
    /// TypeSafe support asks for. `null` until a request was made.
    pub request_id: Option<String>,
    pub telemetry: Telemetry,
}

#[derive(Serialize, Debug)]
pub struct ErrorBody {
    pub kind: &'static str,
    pub message: String,
    pub hint: &'static str,
    pub example: &'static str,
}

#[derive(Serialize, Debug)]
pub struct Envelope<'a> {
    pub ok: bool,
    pub command: &'a str,
    pub version: &'static str,
    pub exit_code: i32,
    pub data: serde_json::Value,
    pub meta: Meta,
    pub error: Option<ErrorBody>,
}

pub fn render(format: Format, env: &Envelope) -> anyhow::Result<String> {
    let v = serde_json::to_value(env)?;
    Ok(match format {
        Format::Json => serde_json::to_string_pretty(&v)?,
        Format::Jsonl => serde_json::to_string(&v)?,
        Format::Toon => {
            toon_format::encode_default(&v).map_err(|e| anyhow::anyhow!(e.to_string()))?
        }
        Format::Human => unreachable!("human output is rendered by each command"),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn presentation_quotes_each_posix_shell_token_as_bytes() {
        use std::os::unix::ffi::OsStringExt;
        assert_eq!(
            super::shell_quote(&[
                "cp".into(),
                "report copy.txt".into(),
                "it's;$HOME".into(),
                "".into(),
                std::ffi::OsString::from_vec(vec![0xff, b'\n']),
            ]),
            b"'cp' 'report copy.txt' 'it'\\''s;$HOME' '' '\xff\n'"
        );
        assert!(super::shell_quote(&[]).is_empty());
    }
    use super::*;
    fn envelope() -> Envelope<'static> {
        Envelope {
            ok: true,
            command: "pick",
            version: "0.0.0",
            exit_code: 0,
            data: serde_json::json!({ "matches": [] }),
            meta: Meta {
                backend: "classifier",
                ..Meta::default()
            },
            error: None,
        }
    }
    #[test]
    fn json_and_jsonl_carry_the_same_envelope() {
        let env = envelope();
        let pretty: serde_json::Value =
            serde_json::from_str(&render(Format::Json, &env).unwrap()).unwrap();
        let line = render(Format::Jsonl, &env).unwrap();
        assert!(!line.contains('\n'), "jsonl is one line");
        assert_eq!(
            pretty,
            serde_json::from_str::<serde_json::Value>(&line).unwrap()
        );
        assert_eq!(pretty["command"], "pick");
        assert_eq!(pretty["error"], serde_json::Value::Null);
        // Which API answered is part of the envelope, not just of `-v` output.
        assert_eq!(pretty["meta"]["backend"], "classifier");
    }
    #[test]
    fn toon_renders_the_fields() {
        let s = render(Format::Toon, &envelope()).unwrap();
        assert!(s.contains("ok: true") && s.contains("command: pick"), "{s}");
    }
}
