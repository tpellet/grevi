use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, JevifyError};
use crate::jev::client::Client;
use crate::records::{self, Split};
use crate::tournament::{Prompts, Ranking, rank, shortlist, window};

/// 100k lines would be 500 requests: a 429 storm and ~25 s. pick ranks a list, it does not scan.
pub const MAX_LINES: usize = 20_000;
const MAX_FINALISTS: usize = 24;

pub async fn run(
    ctx: &Config,
    intent: &str,
    top: usize,
    index: bool,
    split: Split,
    files: bool,
    from: Option<&str>,
) -> Result<Outcome, JevifyError> {
    if from.is_some() {
        return Err(JevifyError::Input("pick --from is not implemented".into()));
    }
    if top == 0 {
        return Err(JevifyError::Usage("-n must be at least 1".into()));
    }
    if files && index {
        return Err(JevifyError::Usage(
            "--index numbers stdin lines; with --files the match is a path".into(),
        ));
    }
    let client = Client::new(ctx)?;
    let bytes = tokio::task::spawn_blocking(crate::input::read_stdin_bytes)
        .await
        .map_err(|e| JevifyError::Input(e.to_string()))??;
    let records = records::parse(&bytes, split)?;
    if records.is_empty() {
        return Err(JevifyError::EmptyInput("stdin was empty"));
    }
    // Blank lines never leave the machine: as empty items they would take window slots and tokens.
    // A repeated line is sent once (the first occurrence keeps its line number): two identical
    // items split the Choice mass between them.
    let (kept, _) = records::distinct(&bytes, &records);
    if kept.len() > MAX_LINES {
        return Err(JevifyError::InputTooLarge(if files {
            format!("more than {MAX_LINES} files; narrow the input list")
        } else {
            format!("more than {MAX_LINES} lines; filter first (rg, head) or split the list")
        }));
    }
    let items: Vec<String> = kept.iter().map(|&i| records[i].evidence.clone()).collect();
    let prompts = if files {
        Prompts {
            choose: "Each entry in `items` is a file: its path, and for some entries the beginning of its content. Which file is the one described by `request`? Choose NONE if no file matches.".into(),
            none: "no file in the list matches the request".into(),
            any: "Is at least one file in `items` the one described by `request`?".into(),
        }
    } else {
        Prompts {
            choose: "Which entry in `items` is the one described by `request`? Choose NONE if no entry matches.".into(),
            none: "no entry in the list matches the request".into(),
            any: "Is at least one entry in `items` the thing described by `request`?".into(),
        }
    };
    let ranking = if files {
        let mut pool = shortlist(&client, intent, &items, &prompts, 3).await?;
        pool.retain(|candidate| candidate.p > 0.0);
        pool.truncate(MAX_FINALISTS);
        if pool.is_empty() {
            Ranking {
                candidates: vec![],
                any: 0.0,
                none: 1.0,
            }
        } else {
            let mut finalists: Vec<_> = pool
                .iter()
                .map(|c| records[kept[c.index]].clone())
                .collect();
            let cwd = std::env::current_dir().map_err(|e| JevifyError::Input(e.to_string()))?;
            let withheld = records::excerpts(&mut finalists, &cwd).await?;
            eprintln!("jevify pick: excerpts withheld: {withheld}");
            let finals: Vec<_> = pool
                .iter()
                .zip(finalists)
                .map(|(c, r)| (c.index, r.evidence))
                .collect();
            window(&client, intent, &finals, &prompts).await?
        }
    } else {
        rank(&client, intent, &items, &prompts, None).await?
    };
    // Found only if the absolute Noul agrees and the best line beats NONE in the Choice.
    let found = ranking.any >= ctx.threshold
        && ranking
            .candidates
            .first()
            .is_some_and(|c| c.p > ranking.none);
    let mut selected: Vec<_> = if found {
        ranking
            .candidates
            .iter()
            .filter(|c| c.p > ranking.none)
            .take(top)
            .collect()
    } else {
        vec![]
    };
    selected.sort_by_key(|candidate| candidate.index);
    let mut human = Vec::new();
    let mut matches = Vec::new();
    for candidate in selected {
        let i = kept[candidate.index];
        let record = &records[i];
        let ordinal = match split {
            Split::Para => i + 1,
            Split::Lines | Split::Nul => {
                let delimiter = if split == Split::Nul { 0 } else { b'\n' };
                bytes[..record.raw.start]
                    .iter()
                    .filter(|&&b| b == delimiter)
                    .count()
                    + 1
            }
        };
        let mut value = records::envelope(&bytes, record, ordinal);
        // Keep the established text field for UTF-8 handles; lossy records use the raw helper.
        if let Some(text) = record.handle.to_str() {
            value["text"] = text.into();
        }
        value["line"] = ordinal.into();
        value["p"] = candidate.p.into();
        matches.push(value);
        if index {
            human.extend_from_slice(format!("{ordinal}\n").as_bytes());
        } else {
            human.extend_from_slice(&bytes[record.raw.clone()]);
        }
    }
    Ok(Outcome {
        exit: if matches.is_empty() {
            Exit::Abstain
        } else {
            Exit::Ok
        },
        data: serde_json::json!({ "matches": matches, "any": ranking.any, "source": if files { "files" } else { "stdin" } }),
        human,
        exec: None,
    })
}
