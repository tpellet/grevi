use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, HunchError};
use crate::jev::client::Client;
use crate::tournament::{Prompts, rank};

/// 100k lines would be 500 requests: a 429 storm and ~25 s. pick ranks a list, it does not scan.
pub const MAX_LINES: usize = 20_000;

pub async fn run(
    ctx: &Config,
    intent: &str,
    top: usize,
    index: bool,
) -> Result<Outcome, HunchError> {
    if top == 0 {
        return Err(HunchError::Usage("-n must be at least 1".into()));
    }
    let client = Client::new(ctx)?;
    let lines = crate::input::read_stdin_async().await?;
    // Blank lines never leave the machine: as empty items they would take window slots and tokens.
    // A repeated line is sent once (the first occurrence keeps its line number): two identical
    // items split the Choice mass between them.
    let mut seen = std::collections::HashSet::new();
    let kept: Vec<usize> = (0..lines.len())
        .filter(|&i| !lines[i].trim().is_empty() && seen.insert(lines[i].trim()))
        .collect();
    if kept.len() > MAX_LINES {
        return Err(HunchError::InputTooLarge(format!(
            "more than {MAX_LINES} lines; filter first (rg, head) or split the list"
        )));
    }
    let items: Vec<String> = kept.iter().map(|&i| lines[i].clone()).collect();
    let prompts = Prompts {
        choose: "Which entry in `items` is the one described by `request`? Choose NONE if no entry matches.".into(),
        none: "no entry in the list matches the request".into(),
        any: "Is at least one entry in `items` the thing described by `request`?".into(),
    };
    let ranking = rank(&client, intent, &items, &prompts, None).await?;
    // Found only if the absolute Noul agrees and the best line beats NONE in the Choice.
    let found = ranking.any >= ctx.threshold
        && ranking
            .candidates
            .first()
            .is_some_and(|c| c.p > ranking.none);
    let matches: Vec<_> = if found {
        ranking
            .candidates
            .iter()
            .filter(|c| c.p > ranking.none)
            .take(top)
            .map(|c| {
                let i = kept[c.index];
                serde_json::json!({ "line": i + 1, "text": lines[i], "p": c.p })
            })
            .collect()
    } else {
        vec![]
    };
    let human: String = matches
        .iter()
        .map(|m| {
            if index {
                format!("{}\n", m["line"])
            } else {
                format!("{}\n", m["text"].as_str().unwrap_or_default())
            }
        })
        .collect();
    Ok(Outcome {
        exit: if matches.is_empty() {
            Exit::Abstain
        } else {
            Exit::Ok
        },
        data: serde_json::json!({ "matches": matches, "any": ranking.any }),
        human,
    })
}
