use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, JevifyError};
use crate::jev::client::Client;
use crate::records::Split;
use crate::tournament::{Prompts, rank};
use std::path::Path;

/// 100k lines would be 500 requests: a 429 storm and ~25 s. pick ranks a list, it does not scan.
pub const MAX_LINES: usize = 20_000;

pub async fn run(
    ctx: &Config,
    intent: &str,
    top: usize,
    index: bool,
    split: Split,
    files: bool,
) -> Result<Outcome, JevifyError> {
    if top == 0 {
        return Err(JevifyError::Usage("-n must be at least 1".into()));
    }
    if files && index {
        return Err(JevifyError::Usage(
            "--index numbers stdin lines; with --files the match is a path".into(),
        ));
    }
    if split != Split::Lines {
        return Err(JevifyError::Input(
            "pick split mode: not implemented".into(),
        ));
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
        return Err(JevifyError::InputTooLarge(if files {
            format!("more than {MAX_LINES} files; narrow the input list")
        } else {
            format!("more than {MAX_LINES} lines; filter first (rg, head) or split the list")
        }));
    }
    let items: Vec<String> = kept.iter().map(|&i| lines[i].clone()).collect();
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
    // Round one ranks path names only. Content leaves the machine for the finalists alone (at
    // most 24 files). The reads block between the two rounds, when no request is in flight.
    let with_content = |k: usize| {
        let excerpt = if visible(&items[k]) {
            std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        let path = Path::new(&items[k]);
                        let parent = path
                            .parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .unwrap_or(Path::new("."));
                        match (parent.canonicalize(), path.file_name()) {
                            (Ok(parent), Some(name)) => {
                                crate::cmd::sort::excerpt(&parent.join(name))
                            }
                            _ => String::new(),
                        }
                    })
                    .join()
                    .unwrap_or_default()
            })
        } else {
            String::new()
        };
        format!("{}\n{excerpt}", items[k])
    };
    let finalist_text: Option<&(dyn Fn(usize) -> String + Sync)> =
        files.then_some(&with_content as _);
    let ranking = rank(&client, intent, &items, &prompts, finalist_text).await?;
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
                let text = lines[i].clone();
                serde_json::json!({ "line": i + 1, "text": text, "p": c.p })
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
        data: serde_json::json!({ "matches": matches, "any": ranking.any, "source": if files { "files" } else { "stdin" } }),
        human: human.into_bytes(),
        exec: None,
    })
}

/// Hidden paths remain candidates by name but receive no excerpt.
fn visible(rel: &str) -> bool {
    !rel.split('/').any(|c| c.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_components_receive_no_excerpt() {
        assert!(visible("src/cmd/pick.rs"));
        assert!(visible("notes.v2.txt"));
        assert!(!visible(".env"));
        assert!(!visible("config/.secrets/key.txt"));
        assert!(!visible(".git/config"));
    }
}
