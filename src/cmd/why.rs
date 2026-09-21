use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, JevifyError};
use crate::jev::client::Client;
use crate::tournament::{Prompts, rank};
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;

pub const SMALL: usize = 1500;
pub const TAIL: usize = 1000;
pub const NEIGHBOURS: usize = 5;
pub const MAX_KEEP: usize = 4000;

static SIGNAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(error|err!|fail(ed|ure|s)?|fatal|panic(ked)?|exception|traceback|denied|not found|no such|cannot|can't|couldn't|undefined|unresolved|refused|timed? ?out|segmentation|abort(ed)?|killed|exit (code|status) [1-9]|assert)").unwrap()
});

pub fn prefilter(lines: &[String]) -> Vec<usize> {
    // Blank lines and repeats never leave the machine: a line already seen (a repeated warning,
    // a separator) would be a second identical item splitting the Choice mass. The first
    // occurrence keeps its line number; context is read from `lines`, so nothing is lost.
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut base: Vec<usize> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim().is_empty() || !seen.insert(l.as_str()) {
            continue;
        }
        base.push(i);
    }
    if base.len() <= SMALL {
        return base;
    }
    // The root cause is usually the first error: keep signal neighbourhoods from the top
    // first, then fill the remaining budget with the tail.
    let mut keep: BTreeSet<usize> = BTreeSet::new();
    for p in (0..base.len()).filter(|&p| SIGNAL.is_match(&lines[base[p]])) {
        if keep.len() >= MAX_KEEP - TAIL {
            break;
        }
        let hi = (p + NEIGHBOURS).min(base.len() - 1);
        keep.extend(&base[p.saturating_sub(NEIGHBOURS)..=hi]);
    }
    for &i in base.iter().rev() {
        if keep.len() >= MAX_KEEP {
            break;
        }
        keep.insert(i);
    }
    keep.into_iter().collect()
}

pub async fn run(
    ctx: &Config,
    context: usize,
    top: usize,
    _no_save: bool,
) -> Result<Outcome, JevifyError> {
    if top == 0 {
        return Err(JevifyError::Usage("-n must be at least 1".into()));
    }
    let client = Client::new(ctx)?;
    let lines = crate::input::read_stdin_async().await?;
    let kept = prefilter(&lines);
    let no_signal = !kept.iter().any(|&i| SIGNAL.is_match(&lines[i]));
    let items: Vec<String> = kept
        .iter()
        .map(|&i| format!("line {}: {}", i + 1, lines[i]))
        .collect();
    let prompts = Prompts {
        choose: "`items` are lines of output from a failed command, build or test run. Which line states the root cause of the failure: the first error that explains why it failed — not a later consequence, a generic summary such as `build failed`, a warning, or a stack frame? Choose NONE if the output shows no failure.".into(),
        none: "the output shows no failure".into(),
        any: "Does `items` contain a line that states why the command, build or test failed?".into(),
    };
    // Neighbours are labelled as context so a literal reader does not pick them as the answer.
    let with_context = |k: usize| {
        let i = kept[k];
        let prev = if i > 0 { lines[i - 1].as_str() } else { "" };
        let next = lines.get(i + 1).map(String::as_str).unwrap_or("");
        format!(
            "line {}: {}\n    (context only, not a candidate; before: {prev})\n    (context only, not a candidate; after: {next})",
            i + 1,
            lines[i]
        )
    };
    let ranking = rank(
        &client,
        "find the root cause of the failure",
        &items,
        &prompts,
        Some(&with_context),
    )
    .await?;
    let found = ranking.any >= ctx.threshold
        && ranking
            .candidates
            .first()
            .is_some_and(|c| c.p > ranking.none);
    let causes: Vec<serde_json::Value> = if found {
        ranking
            .candidates
            .iter()
            .filter(|c| c.p > ranking.none)
            .take(top)
            .map(|c| {
                let i = kept[c.index];
                let lo = i.saturating_sub(context);
                let hi = (i + context).min(lines.len() - 1);
                let ctx_lines: Vec<_> = (lo..=hi).map(|j| serde_json::json!({ "line": j + 1, "text": lines[j] })).collect();
                serde_json::json!({ "line": i + 1, "text": lines[i], "p": c.p, "context": ctx_lines })
            })
            .collect()
    } else {
        vec![]
    };
    let mut human = String::new();
    for c in &causes {
        let line = c["line"].as_u64().unwrap_or(0);
        for l in c["context"].as_array().into_iter().flatten() {
            let n = l["line"].as_u64().unwrap_or(0);
            let mark = if n == line { ">" } else { " " };
            human.push_str(&format!(
                "{mark}{n:>6} │ {}\n",
                l["text"].as_str().unwrap_or_default()
            ));
        }
        human.push('\n');
    }
    // The first thing most people get wrong: compilers write errors to stderr.
    let hint = (causes.is_empty() && no_signal).then(|| {
        "no error-like lines on stdin; most tools write errors to stderr: `cmd 2>&1 | jevify why`".to_string()
    });
    if let Some(h) = &hint {
        eprintln!("jevify why: {h}");
    }
    Ok(Outcome {
        exit: if causes.is_empty() {
            Exit::Abstain
        } else {
            Exit::Ok
        },
        data: serde_json::json!({ "causes": causes, "any": ranking.any, "considered": kept.len(), "total": lines.len(), "hint": hint }),
        human: human.into_bytes(),
        exec: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_logs_keep_everything_nonblank_and_dedupe_repeats() {
        // Blank lines, a run of repeats and a later repeat of `a` are all dropped; the first
        // occurrence keeps its index.
        let lines: Vec<String> = ["a", "", "b", "b", "b", "c", "a"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(prefilter(&lines), vec![0, 2, 5]);
    }
    #[test]
    fn large_logs_keep_tail_and_signal_neighbourhoods() {
        let mut lines: Vec<String> = (0..20_000)
            .map(|i| format!("compiling crate {i}"))
            .collect();
        lines[100] = "error[E0432]: unresolved import `foo`".into();
        let kept = prefilter(&lines);
        assert!(kept.contains(&100) && kept.contains(&95) && kept.contains(&105));
        assert!(kept.contains(&19_999));
        assert!(kept.len() <= MAX_KEEP);
    }
    #[test]
    fn earliest_error_survives_many_later_signals() {
        let mut lines: Vec<String> = (0..50_000)
            .map(|i| {
                if i > 1_000 && i % 7 == 0 {
                    format!("npm ERR! error {i}")
                } else {
                    format!("step {i}")
                }
            })
            .collect();
        lines[10] = "error: linker `cc` not found".into();
        let kept = prefilter(&lines);
        assert!(kept.contains(&10) && kept.contains(&49_999));
        assert!(kept.len() <= MAX_KEEP);
    }
}
