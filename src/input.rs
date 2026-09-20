use crate::exit::GreviError;
use regex::Regex;
use std::io::{IsTerminal, Read};
use std::sync::LazyLock;

pub const MAX_BYTES: usize = 64 * 1024 * 1024;

static ANSI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)").unwrap()
});

static SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)((?:(?:api[_-]?key|token|secret|passw(?:or)?d|authorization)\b["']?\s*[:=]\s*["']?|\bbearer\s+))[^\s"',;]{8,}|\b(?:sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9-]{10,}|eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,})"#).unwrap()
});

/// Redact semantic text while preserving object keys and non-text values.
pub fn redact_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(redact(s)),
        serde_json::Value::Array(values) => values.iter().map(redact_value).collect(),
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(key, value)| (key.clone(), redact_value(value)))
            .collect(),
        other => other.clone(),
    }
}

pub fn strip_ansi(s: &str) -> String {
    ANSI.replace_all(s, "").into_owned()
}

/// Best-effort masking of obvious secrets in text that is about to leave the machine (CI logs
/// routinely echo tokens). Not a guarantee; PRIVACY.md says so. Local output keeps the original.
pub fn redact(s: &str) -> String {
    SECRET
        .replace_all(s, |c: &regex::Captures| {
            format!("{}[REDACTED]", c.get(1).map_or("", |m| m.as_str()))
        })
        .into_owned()
}

pub fn split_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            let l = l.rsplit('\r').next().unwrap_or(l);
            strip_ansi(l).trim_end().to_string()
        })
        .collect()
}

pub fn read_stdin() -> Result<Vec<String>, GreviError> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err(GreviError::EmptyInput("pipe text into grevi"));
    }
    let mut buf = Vec::new();
    stdin
        .by_ref()
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| GreviError::Input(e.to_string()))?;
    if buf.len() > MAX_BYTES {
        return Err(GreviError::InputTooLarge(format!(
            "more than {} MiB on stdin",
            MAX_BYTES / 1024 / 1024
        )));
    }
    let lines = split_lines(&String::from_utf8_lossy(&buf));
    if lines.iter().all(|l| l.trim().is_empty()) {
        return Err(GreviError::EmptyInput("stdin was empty"));
    }
    Ok(lines)
}

/// Reads stdin on the blocking pool so a slow producer never stalls the runtime.
pub async fn read_stdin_async() -> Result<Vec<String>, GreviError> {
    tokio::task::spawn_blocking(read_stdin)
        .await
        .map_err(|e| GreviError::Input(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_ansi_and_carriage_returns() {
        let got = split_lines("\x1b[31merror\x1b[0m: boom\n 10%\r 50%\r100% done  \n");
        assert_eq!(
            got,
            vec!["error: boom".to_string(), "100% done".to_string()]
        );
    }
    #[test]
    fn redacts_obvious_secrets_only() {
        assert_eq!(
            redact("token_expiry_seconds = 3600"),
            "token_expiry_seconds = 3600"
        );
        assert_eq!(
            redact("export GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123"),
            "export GITHUB_TOKEN=[REDACTED]"
        );
        assert_eq!(
            redact("Authorization: Bearer abcdefghijklmnop"),
            "Authorization: Bearer [REDACTED]"
        );
        assert_eq!(
            redact("error[E0432]: unresolved import `foo`"),
            "error[E0432]: unresolved import `foo`"
        );
    }
}
