//! `label a,b,c`: tag each stdin record with one of the caller's labels.
//!
//! Output is `LABEL<TAB>RECORD` in input order, `?` for an unsure record. The labels are
//! validated by the parser (`cli::Labels`); their count against the backend's window is
//! checked here, after `Config::load`, where the backend is known.
//!
//! One Choice per record, the labels plus an internal `NONE`, through the scorer of `filter`.
//! The `?` decision is `tournament::decide` over a `Ranking` built from the record's answer
//! with `any = 1.0`, so the threshold plays no part: `NONE` winning or tying the best label
//! and the winner ratio do. `label` saves nothing: every record comes out.

use crate::{
    cmd::{
        Outcome,
        filter::{batch_size, score, write_record},
    },
    config::Config,
    exit::{Exit, JevifyError},
    jev::{Question, Questions, client::Client},
    records::{self, Split},
    tournament::{Candidate, Decision, Ranking, decide},
};
use std::collections::BTreeMap;

pub struct LabelFlags {
    pub split: Split,
    pub files: bool,
}

const UNSURE: &str = "?";

pub async fn run(
    ctx: &Config,
    labels: Vec<String>,
    flags: LabelFlags,
    machine: bool,
) -> Result<Outcome, JevifyError> {
    let window = ctx.backend.window();
    if labels.len() > window {
        return Err(JevifyError::Usage(format!(
            "{} labels given; at most {window} on the {} backend",
            labels.len(),
            ctx.backend.as_str()
        )));
    }
    let input = tokio::task::spawn_blocking(crate::input::read_stdin_bytes)
        .await
        .map_err(|e| JevifyError::Input(e.to_string()))??;
    let mut records = records::parse(&input, flags.split)?;
    let (unique, occurrences) = records::distinct(&input, &records);
    if unique.len() > 20_000 {
        return Err(JevifyError::Kinded {
            kind: "too_many",
            exit: Exit::Input,
            message: "more than 20,000 distinct records; narrow with grep or head".into(),
            hint: "narrow with grep or head",
            example: "head -n 20000 input | jevify label bug,feature",
        });
    }
    let withheld = if flags.files {
        let cwd = std::env::current_dir().map_err(|e| JevifyError::Input(e.to_string()))?;
        records::excerpts(&mut records, &cwd).await?
    } else {
        0
    };
    if !machine && withheld > 0 {
        eprintln!("jevify label: excerpts withheld: {withheld}");
    }
    let client = Client::new(ctx)?;
    let evidence: Vec<_> = unique
        .iter()
        .map(|&i| records[i].evidence.clone())
        .collect();
    let questions = Questions::from([("label".into(), question(&labels))]);
    let size = batch_size(client.backend(), questions.len());
    eprintln!(
        "jevify label: {} records, {} distinct, {} requests",
        records.len(),
        unique.len(),
        unique.len().div_ceil(size)
    );
    let mut answers: Vec<(String, f64)> = Vec::with_capacity(unique.len());
    let mut cursor = 0;
    let mut labelled = 0;
    let mut unsure = 0;
    let mut entries = Vec::new();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let result = score(&client, &evidence, &questions, |batch| {
        for response in batch {
            answers.push(verdict(&labels, response.probs("label")?, ctx.threshold));
        }
        while cursor < records.len() && occurrences[cursor] < answers.len() {
            let (label, p) = &answers[occurrences[cursor]];
            if label == UNSURE {
                unsure += 1;
            } else {
                labelled += 1;
            }
            if machine {
                let mut entry = records::envelope(&input, &records[cursor], cursor + 1);
                entry["label"] = label.as_str().into();
                entry["p"] = (*p).into();
                entries.push(entry);
            } else {
                let mut line = Vec::with_capacity(label.len() + 1 + records[cursor].raw.len());
                line.extend_from_slice(label.as_bytes());
                line.push(b'\t');
                line.extend_from_slice(&input[records[cursor].raw.clone()]);
                if !write_record(&mut stdout, &line)? {
                    return Ok(false);
                }
            }
            cursor += 1;
        }
        Ok(true)
    })
    .await;
    let completed = match result {
        Ok(completed) => completed,
        Err(error) => {
            let answered = format!("answered {} of {}", answers.len(), unique.len());
            if machine {
                return Err(JevifyError::Kinded {
                    kind: error.kind(),
                    exit: error.exit(),
                    message: format!("{error}; {answered}"),
                    hint: error.hint(),
                    example: error.example(),
                });
            }
            eprintln!("jevify label: {answered}");
            return Err(error);
        }
    };
    let exit = if completed && !records.is_empty() && unsure == records.len() {
        Exit::Abstain
    } else {
        Exit::Ok
    };
    let mut status = format!(
        "jevify label: labelled {labelled} of {}, {unsure} unsure",
        records.len()
    );
    if withheld > 0 {
        status.push_str(&format!(", excerpts withheld: {withheld}"));
    }
    if let Some(model) = ctx.stats.model.lock().unwrap().as_ref() {
        let others: Vec<_> = model
            .split(", ")
            .filter(|m| !m.starts_with("jev"))
            .collect();
        if !others.is_empty() {
            status.push_str(&format!(", answered by {}, not Jev", others.join(", ")));
        }
    }
    eprintln!("{}", status.replace(['\r', '\n'], " "));
    Ok(Outcome {
        exit,
        data: serde_json::json!({
            "records": entries, "labelled": labelled, "total": records.len(), "unsure": unsure,
            "complete": completed, "excerpts_withheld": withheld
        }),
        human: Vec::new(),
        exec: None,
    })
}

/// The one Choice: the caller's labels, and NONE for a record none of them fits.
fn question(labels: &[String]) -> Question {
    let mut criteria: BTreeMap<String, Option<String>> =
        labels.iter().map(|l| (l.clone(), None)).collect();
    criteria.insert(
        "NONE".into(),
        Some("none of the labels fits this record".into()),
    );
    Question::choice(
        format!(
            "judge this one record alone and give it the one label that fits it best: {}",
            labels.join(", ")
        ),
        criteria,
    )
}

/// The label of one record and the probability behind it: the winner's, or the best label's
/// when the answer is `?`.
fn verdict(labels: &[String], probs: &BTreeMap<String, f64>, threshold: f64) -> (String, f64) {
    let mut candidates: Vec<Candidate> = labels
        .iter()
        .enumerate()
        .map(|(index, label)| Candidate {
            index,
            p: probs.get(label).copied().unwrap_or(0.0),
        })
        .collect();
    candidates.sort_by(|a, b| b.p.total_cmp(&a.p));
    let best = candidates.first().map_or(0.0, |c| c.p);
    let ranking = Ranking {
        candidates,
        any: 1.0,
        none: probs.get("NONE").copied().unwrap_or(0.0),
        windows: 1,
        n: labels.len(),
    };
    match decide(&ranking, threshold) {
        Decision::Found(winner) => (labels[winner.index].clone(), winner.p),
        Decision::NoMatch | Decision::Ambiguous(_) => (UNSURE.into(), best),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probs(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, p)| (k.to_string(), *p)).collect()
    }

    #[test]
    fn verdict_needs_a_clear_winner_over_the_runner_up_and_none() {
        let labels = ["bug".to_string(), "feature".to_string()];
        for (answer, threshold, expected) in [
            (
                probs(&[("bug", 0.8), ("feature", 0.1), ("NONE", 0.1)]),
                0.5,
                ("bug", 0.8),
            ),
            (
                probs(&[("bug", 0.8), ("feature", 0.1), ("NONE", 0.1)]),
                0.99,
                ("bug", 0.8),
            ),
            (
                probs(&[("bug", 0.45), ("feature", 0.45), ("NONE", 0.1)]),
                0.5,
                ("?", 0.45),
            ),
            (
                probs(&[("bug", 0.5), ("feature", 0.4), ("NONE", 0.1)]),
                0.5,
                ("?", 0.5),
            ),
            (
                probs(&[("bug", 0.2), ("feature", 0.1), ("NONE", 0.7)]),
                0.5,
                ("?", 0.2),
            ),
            (
                probs(&[("bug", 0.4), ("feature", 0.2), ("NONE", 0.4)]),
                0.5,
                ("?", 0.4),
            ),
            (probs(&[("NONE", 1.0)]), 0.5, ("?", 0.0)),
        ] {
            let (label, p) = verdict(&labels, &answer, threshold);
            assert_eq!((label.as_str(), p), expected, "{answer:?}");
        }
        assert_eq!(verdict(&[], &probs(&[("NONE", 0.0)]), 0.5).0, "?");
    }

    #[test]
    fn question_lists_every_label_and_none() {
        let labels = ["bug".to_string(), "feature".to_string()];
        let asked = serde_json::to_value(question(&labels)).unwrap();
        assert_eq!(asked["type"], "choice");
        assert!(
            asked["instructions"]
                .as_str()
                .unwrap()
                .contains("bug, feature")
        );
        let criteria = asked["criteria"].as_object().unwrap();
        assert_eq!(
            criteria.keys().collect::<Vec<_>>(),
            ["NONE", "bug", "feature"]
        );
        assert!(criteria["NONE"].is_string());
        assert!(criteria["bug"].is_null());
    }
}
