use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, JevifyError};
use crate::jev::client::Client;
use crate::jev::{Question, Questions};

/// ~24k tokens of typical text at ~4 chars/token; keeps state + question under the 32k limit.
/// Denser text (non-Latin scripts) can exceed it and surfaces as `api_rejected_request` (exit 6).
const MAX_CHARS: usize = 96_000;

pub async fn run(
    ctx: &Config,
    statements: &[String],
    context: Option<&std::path::Path>,
    band: f64,
) -> Result<Outcome, JevifyError> {
    let [condition] = statements else {
        return Err(JevifyError::Input(
            "several statements: not implemented".into(),
        ));
    };
    if context.is_some() {
        return Err(JevifyError::Input("--context: not implemented".into()));
    }
    // Above 0.5 the "no" verdict becomes unreachable at the default threshold.
    if !(0.0..=0.5).contains(&band) {
        return Err(JevifyError::Usage(format!(
            "--band {band} must be within 0..=0.5"
        )));
    }
    let client = Client::new(ctx)?;
    let lines = crate::input::read_stdin_async().await?;
    let text = crate::input::redact(&lines.join("\n"));
    let max_chars = MAX_CHARS.min(client.backend().max_state_chars());
    // Backend evidence budgets count Unicode characters, not UTF-8 bytes.
    let truncated = text.chars().count() > max_chars;
    if truncated {
        eprintln!("jevify is: input exceeds the evidence budget; whole input not judged");
        return Ok(Outcome {
            exit: Exit::Abstain,
            data: serde_json::json!({ "p": null, "verdict": "unsure", "truncated": true, "reason": "input exceeds the evidence budget; whole input not judged" }),
            human: Vec::new(),
            exec: None,
        });
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
        human: Vec::new(),
        exec: None,
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
