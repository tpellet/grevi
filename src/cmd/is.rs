use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, HunchError};
use crate::jev::client::Client;
use crate::jev::{Question, Questions};

/// ~24k tokens of typical text at ~4 chars/token; keeps state + question under the 32k limit.
/// Denser text (non-Latin scripts) can exceed it and surfaces as `api_rejected_request` (exit 6).
const MAX_CHARS: usize = 96_000;

pub async fn run(ctx: &Config, condition: &str, band: f64) -> Result<Outcome, HunchError> {
    // Above 0.5 the "no" verdict becomes unreachable at the default threshold.
    if !(0.0..=0.5).contains(&band) {
        return Err(HunchError::Usage(format!(
            "--band {band} must be within 0..=0.5"
        )));
    }
    let client = Client::new(ctx)?;
    let lines = crate::input::read_stdin_async().await?;
    let mut text = crate::input::redact(&lines.join("\n"));
    // Counted in chars, like the slicing below: bytes would under-truncate multi-byte text.
    let truncated = text.chars().count() > MAX_CHARS;
    if truncated {
        // keep head and tail: conditions are usually decided by the start or the end of a text
        let head: String = text.chars().take(MAX_CHARS / 2).collect();
        let tail: String = text
            .chars()
            .rev()
            .take(MAX_CHARS / 2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        text = format!("{head}\n[... truncated by hunch ...]\n{tail}");
    }
    let mut qs = Questions::new();
    qs.insert(
        "is".into(),
        Question::noul_with(
            format!("Does the text in the state satisfy this condition: \"{condition}\"?"),
            "The text clearly satisfies the condition",
            "The text does not satisfy the condition",
        ),
    );
    let p = client
        .ask(&serde_json::Value::String(text), &qs)
        .await?
        .noul("is")?;
    let (exit, verdict) = band_verdict(p, ctx.threshold, band);
    Ok(Outcome {
        exit,
        data: serde_json::json!({ "p": p, "verdict": verdict, "truncated": truncated }),
        human: String::new(),
    })
}

/// yes at or above `threshold + band`, no below `threshold - band`, unsure in between.
pub fn band_verdict(p: f64, threshold: f64, band: f64) -> (Exit, &'static str) {
    if p >= (threshold + band).min(1.0) {
        (Exit::Ok, "yes")
    } else if p < (threshold - band).max(0.0) {
        (Exit::No, "no")
    } else {
        (Exit::Abstain, "unsure")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verdict_bands_around_the_threshold() {
        let cases = [
            (0.9, 0.5, 0.15, Exit::Ok),
            (0.65, 0.5, 0.15, Exit::Ok),
            (0.64, 0.5, 0.15, Exit::Abstain),
            (0.35, 0.5, 0.15, Exit::Abstain),
            (0.34, 0.5, 0.15, Exit::No),
            // band 0: a plain threshold, no unsure verdict
            (0.5, 0.5, 0.0, Exit::Ok),
            (0.49, 0.5, 0.0, Exit::No),
            // the bands are clamped to 0..=1
            (1.0, 1.0, 0.15, Exit::Ok),
            (0.0, 0.0, 0.15, Exit::Abstain),
        ];
        for (p, t, band, exit) in cases {
            assert_eq!(band_verdict(p, t, band).0, exit, "p={p} t={t} band={band}");
        }
        assert_eq!(band_verdict(0.9, 0.5, 0.15).1, "yes");
        assert_eq!(band_verdict(0.5, 0.5, 0.15).1, "unsure");
        assert_eq!(band_verdict(0.1, 0.5, 0.15).1, "no");
    }
}
