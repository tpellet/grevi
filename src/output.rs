use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Human,
    Json,
    Jsonl,
    Toon,
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct Meta {
    pub model: Option<String>,
    pub elapsed_ms: u128,
    pub requests: u32,
    pub cache_hits: u32,
    pub input_tokens: u64,
    pub cost_usd: f64,
    pub threshold: f64,
    /// `x-typesafe-request-id` of the last TypeSafe response seen (success or failure); what
    /// TypeSafe support asks for. `null` until a request was made.
    pub request_id: Option<String>,
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
    use super::*;
    fn envelope() -> Envelope<'static> {
        Envelope {
            ok: true,
            command: "pick",
            version: "0.0.0",
            exit_code: 0,
            data: serde_json::json!({ "matches": [] }),
            meta: Meta::default(),
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
    }
    #[test]
    fn toon_renders_the_fields() {
        let s = render(Format::Toon, &envelope()).unwrap();
        assert!(s.contains("ok: true") && s.contains("command: pick"), "{s}");
    }
}
