use crate::{
    cmd::Outcome,
    config::Config,
    exit::{Exit, JevifyError},
    jev::{Question, Questions, Response, client::Client},
    records::{self, Split},
};
use futures::StreamExt;
use std::{collections::BTreeMap, io::Write};

pub struct FilterFlags {
    pub invert: bool,
    pub count: bool,
    pub strict: bool,
    pub split: Split,
    pub files: bool,
    pub no_save: bool,
}

pub async fn run(
    ctx: &Config,
    statement: &str,
    flags: FilterFlags,
    machine: bool,
) -> Result<Outcome, JevifyError> {
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
            example: "head -n 20000 input | jevify filter 'x'",
        });
    }
    let directory = crate::config::saved_input_dir(flags.no_save);
    let (input, saved) = tokio::task::spawn_blocking(move || {
        let saved = crate::save::save(&input, directory.as_deref());
        (input, saved)
    })
    .await
    .map_err(|e| JevifyError::Input(e.to_string()))?;
    let unread = if flags.files {
        let cwd = std::env::current_dir().map_err(|e| JevifyError::Input(e.to_string()))?;
        records::excerpts(&mut records, &cwd).await?
    } else {
        records::Unread::default()
    };
    let withheld = unread.count;
    if !machine && withheld > 0 {
        eprintln!("jevify filter: excerpts withheld: {withheld}");
    }
    unread.report("filter", &records);
    let client = Client::new(ctx)?;
    // An unreadable file is never asked about: its name alone is not the evidence the caller
    // asked for, and it comes out unsure with p 0, kept unless --strict.
    let asked: Vec<usize> = (0..unique.len())
        .filter(|&u| !unread.unreadable.contains_key(&unique[u]))
        .collect();
    let evidence: Vec<_> = asked
        .iter()
        .map(|&u| records[unique[u]].evidence.clone())
        .collect();
    let questions = Questions::from([("filter".into(), question(statement))]);
    let size = batch_size(client.backend(), questions.len());
    eprintln!(
        "jevify filter: {} records, {} distinct, {} requests",
        records.len(),
        unique.len(),
        asked.len().div_ceil(size)
    );
    let mut answers: Vec<(f64, &str)> = Vec::with_capacity(unique.len());
    let mut asked_done = 0;
    let mut cursor = 0;
    let mut kept = 0;
    let mut unsure = 0;
    let mut entries = Vec::new();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let mut emit = |answers: &[(f64, &str)]| -> Result<bool, JevifyError> {
        while cursor < records.len() && occurrences[cursor] < answers.len() {
            let (p, verdict) = answers[occurrences[cursor]];
            unsure += usize::from(verdict == "unsure");
            let keep = selected(verdict, flags.invert, flags.strict);
            if keep {
                kept += 1;
                if machine {
                    let mut entry = records::envelope(&input, &records[cursor], cursor + 1);
                    entry["p"] = p.into();
                    entry["verdict"] = verdict.into();
                    if let Some(reason) = unread.unreadable.get(&cursor) {
                        entry["unreadable"] = reason.as_str().into();
                    }
                    entries.push(entry);
                } else if !flags.count
                    && !write_record(&mut stdout, &input[records[cursor].raw.clone()])?
                {
                    return Ok(false);
                }
            }
            cursor += 1;
        }
        Ok(true)
    };
    let result = score(&client, &evidence, &questions, |batch| {
        for response in batch {
            let scores = response.probs("filter")?;
            let (p, fails, silent) = (scores[HOLDS], scores[FAILS], scores[SILENT]);
            // The three sides of the Choice, so a "no" can be read back from the envelope.
            ctx.stats.gate(crate::output::Gate {
                any: Some(p),
                none: Some(silent),
                fails: Some(fails),
                ..Default::default()
            });
            let verdict = verdict(p, fails, ctx.threshold, 0.15);
            answers.resize(asked[asked_done], (0.0, "unsure"));
            answers.push((p, verdict));
            asked_done += 1;
        }
        emit(&answers)
    })
    .await;
    let result = match result {
        Ok(true) => {
            answers.resize(unique.len(), (0.0, "unsure"));
            emit(&answers)
        }
        other => other,
    };
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
            eprintln!("jevify filter: {answered}");
            return Err(error);
        }
    };
    let mut complete = completed && saved.is_ok();
    let mut exit = if !completed {
        Exit::Ok
    } else if !records.is_empty() && unsure == records.len() {
        Exit::Abstain
    } else if kept == 0 {
        Exit::No
    } else {
        Exit::Ok
    };
    if !machine && flags.count && !write_record(&mut stdout, format!("{kept}\n").as_bytes())? {
        exit = Exit::Ok;
        complete = false;
    }
    let saved_description = match &saved {
        Ok(path) => path.display().to_string(),
        Err(reason) => format!("not saved ({reason})"),
    };
    let mut status = format!(
        "jevify filter: kept {kept} of {}, {unsure} unsure, full output: {saved_description}",
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
            "records": entries, "kept": kept, "total": records.len(), "unsure": unsure,
            "complete": complete, "saved_input": saved.ok(), "excerpts_withheld": withheld
        }),
        human: Vec::new(),
        exec: None,
    })
}

pub(crate) const HOLDS: &str = "the record says the statement holds";
pub(crate) const FAILS: &str = "the record says the statement does not hold";
pub(crate) const SILENT: &str = "the record does not say";

/// Three answers, not two: a record that says nothing either way is neither a yes nor a no.
/// A Noul reads "not stated" as a confident no (measured 2026-09-22 on both backends: merge
/// subjects under "the change is a bug fix" scored 0.00–0.17), so the third option takes that
/// mass and the band can see it.
pub(crate) fn question(statement: &str) -> Question {
    Question::choice(
        format!("judge this one record on its own: {statement}"),
        [
            (
                HOLDS.into(),
                Some("the record shows that the statement is true of it".into()),
            ),
            (
                FAILS.into(),
                Some(
                    "the record shows that the statement is false of it: it says the opposite, \
                     or it is about something else"
                        .into(),
                ),
            ),
            (
                SILENT.into(),
                Some(
                    "the record has no content to judge by: a bare reference such as a number, \
                     a name or a merge line"
                        .into(),
                ),
            ),
        ]
        .into(),
    )
}

/// One-sided: yes when P(holds) is at or above `threshold + band`, no when P(fails) is at or
/// above the same mark, unsure otherwise: below the mark on both sides, or mostly unstated.
/// Nothing is compared to `threshold - band`; the default mark is 0.5 + 0.15 = 0.65.
pub(crate) fn verdict(holds: f64, fails: f64, threshold: f64, band: f64) -> &'static str {
    let mark = (threshold + band).min(1.0);
    if holds >= mark {
        "yes"
    } else if fails >= mark {
        "no"
    } else {
        "unsure"
    }
}

fn selected(verdict: &str, invert: bool, strict: bool) -> bool {
    if verdict == "unsure" {
        !strict
    } else {
        (verdict == "yes") != invert
    }
}

pub(crate) use crate::jev::client::batch_size;

/// Delivers batches in record order. Returning false cancels pending requests.
pub(crate) async fn score(
    client: &Client,
    records: &[String],
    questions: &Questions,
    mut emit: impl FnMut(&[Response]) -> Result<bool, JevifyError>,
) -> Result<bool, JevifyError> {
    let mut stream = client.ask_each(records, questions);
    let mut ready = BTreeMap::new();
    let mut cursor = 0;
    while let Some(batch) = stream.next().await {
        let (index, responses) = batch?;
        ready.insert(index, responses);
        while let Some(responses) = ready.remove(&cursor) {
            if !emit(&responses)? {
                return Ok(false);
            }
            cursor += 1;
        }
    }
    Ok(true)
}

/// Flush each record so even a short first batch reaches the downstream reader.
pub(crate) fn write_record(
    stdout: &mut std::io::StdoutLock<'_>,
    bytes: &[u8],
) -> Result<bool, JevifyError> {
    match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(false),
        Err(error) => Err(JevifyError::Input(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Backend;

    #[test]
    fn a_record_that_says_nothing_is_unsure_not_no() {
        assert_eq!(verdict(0.9, 0.05, 0.5, 0.15), "yes");
        assert_eq!(verdict(0.65, 0.3, 0.5, 0.15), "yes");
        assert_eq!(verdict(0.05, 0.9, 0.5, 0.15), "no");
        assert_eq!(verdict(0.3, 0.65, 0.5, 0.15), "no");
        // says nothing: the third option holds the mass
        assert_eq!(verdict(0.1, 0.1, 0.5, 0.15), "unsure");
        // split between the two sides, neither at the mark
        assert_eq!(verdict(0.5, 0.5, 0.5, 0.15), "unsure");
        assert_eq!(verdict(0.64, 0.36, 0.5, 0.15), "unsure");
        // band 0: the threshold alone
        assert_eq!(verdict(0.5, 0.4, 0.5, 0.0), "yes");
        assert_eq!(verdict(0.4, 0.5, 0.5, 0.0), "no");
        // clamped at 1
        assert_eq!(verdict(1.0, 0.0, 1.0, 0.15), "yes");
        assert_eq!(verdict(0.0, 1.0, 1.0, 0.15), "no");
        assert!(matches!(
            question("x"),
            Question::Choice { criteria, .. } if criteria.len() == 3 && criteria.contains_key(SILENT)
        ));
    }

    #[test]
    fn selection_keeps_doubt_in_both_directions_unless_strict() {
        for invert in [false, true] {
            for strict in [false, true] {
                assert_eq!(selected("yes", invert, strict), !invert);
                assert_eq!(selected("no", invert, strict), invert);
                assert_eq!(selected("unsure", invert, strict), !strict);
            }
        }
        assert_eq!(batch_size(Backend::Classifier, 1), 60);
        assert_eq!(batch_size(Backend::Classifier, 20), 3);
        assert_eq!(batch_size(Backend::Classifier, 61), 1);
        assert_eq!(batch_size(Backend::Typesafe, 1), 20);
    }
}
