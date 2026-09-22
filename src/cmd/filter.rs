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
    let directory = if flags.no_save {
        None
    } else {
        crate::config::save_dir(std::env::var("JEVIFY_CACHE_DIR").ok().as_deref())
    };
    let (input, saved) = tokio::task::spawn_blocking(move || {
        let saved = crate::save::save(&input, directory.as_deref());
        (input, saved)
    })
    .await
    .map_err(|e| JevifyError::Input(e.to_string()))?;
    let withheld = if flags.files {
        let cwd = std::env::current_dir().map_err(|e| JevifyError::Input(e.to_string()))?;
        records::excerpts(&mut records, &cwd).await?
    } else {
        0
    };
    if !machine && withheld > 0 {
        eprintln!("jevify filter: excerpts withheld: {withheld}");
    }
    let client = Client::new(ctx)?;
    let evidence: Vec<_> = unique
        .iter()
        .map(|&i| records[i].evidence.clone())
        .collect();
    let questions = Questions::from([(
        "filter".into(),
        Question::noul_with(
            format!("judge this one record: {statement}"),
            "the statement holds",
            "the statement does not hold",
        ),
    )]);
    let size = batch_size(client.backend(), questions.len());
    eprintln!(
        "jevify filter: {} records, {} distinct, {} requests",
        records.len(),
        unique.len(),
        unique.len().div_ceil(size)
    );
    let mut answers = Vec::with_capacity(unique.len());
    let mut cursor = 0;
    let mut kept = 0;
    let mut unsure = 0;
    let mut entries = Vec::new();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let result = score(&client, &evidence, &questions, |batch| {
        for response in batch {
            let p = response.noul("filter")?;
            let (_, verdict) = super::is::band_verdict(p, ctx.threshold, 0.15);
            answers.push((p, verdict));
        }
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
