use crate::exit::GreviError;
use crate::jev::client::Client;
use crate::jev::{Question, Questions};
use std::collections::BTreeMap;

/// The TypeSafe window, and the largest one grevi ever sends. classifier.dev takes 99 options
/// plus NONE; the size in force comes from the backend, not from here.
pub const WINDOW: usize = 200;
const PER_WINDOW_FINALISTS: usize = 3;
const MAX_FINALISTS: usize = 24;
/// Character budget for all items of one window: ~15k tokens at ~4 chars/token (typical text),
/// ~24k for dense logs (hashes, paths, JSON) at ~2.5 chars/token, still under the 32k-token
/// state + question limit. Non-Latin scripts tokenize denser still; the API then answers
/// 413/422, which surfaces as `api_rejected_request` (exit 6), never as corruption.
const WINDOW_CHARS: usize = 60_000;

/// Truncates on a char boundary; long log/JSON lines would otherwise overflow the token budget.
pub fn clip(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    match s.char_indices().nth(max) {
        Some(_) => format!("{}…", s.chars().take(max - 1).collect::<String>()),
        None => s.to_string(),
    }
}

pub struct Prompts {
    /// Choice instruction; must refer to `request` and `items`.
    pub choose: String,
    /// Description of the NONE option.
    pub none: String,
    /// Absolute yes/no instruction: does any item satisfy the request?
    pub any: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub index: usize,
    pub p: f64,
}

#[derive(Debug, Clone)]
pub struct Ranking {
    pub candidates: Vec<Candidate>,
    pub any: f64,
    /// P(NONE) in the deciding Choice; a candidate that does not beat it is not a match.
    pub none: f64,
}

fn id(i: usize) -> String {
    format!("L{i:03}")
}

async fn window(
    client: &Client,
    request: &str,
    items: &[(usize, String)],
    prompts: &Prompts,
) -> Result<Ranking, GreviError> {
    // The backend's own input limit caps the window budget: classifier.dev rejects an input
    // over 32,000 characters, so its windows carry shorter excerpts, never a rejected request.
    let budget = WINDOW_CHARS.min(client.backend().max_state_chars());
    let per_item = (budget / items.len().max(1)).clamp(200, 2_000);
    let state = serde_json::json!({
        "request": request,
        "items": items.iter().enumerate().map(|(i, (_, t))| format!("[{}] {}", id(i), clip(&crate::input::redact(t), per_item))).collect::<Vec<_>>(),
    });
    let mut crit: BTreeMap<String, Option<String>> =
        (0..items.len()).map(|i| (id(i), None)).collect();
    crit.insert("NONE".into(), Some(prompts.none.clone()));
    let mut qs = Questions::new();
    qs.insert(
        "pick".into(),
        Question::choice(prompts.choose.clone(), crit),
    );
    qs.insert("any".into(), Question::noul(prompts.any.clone()));
    let r = client.ask(&state, &qs).await?;
    let probs = r.probs("pick")?;
    let none = probs.get("NONE").copied().unwrap_or(0.0);
    let mut candidates: Vec<Candidate> = probs
        .iter()
        .filter(|(k, _)| k.as_str() != "NONE")
        .filter_map(|(k, p)| {
            k.strip_prefix('L')?
                .parse::<usize>()
                .ok()
                .filter(|i| id(*i) == *k)
                .and_then(|i| items.get(i))
                .map(|(g, _)| Candidate { index: *g, p: *p })
        })
        .collect();
    candidates.sort_by(|a, b| b.p.total_cmp(&a.p));
    Ok(Ranking {
        candidates,
        any: r.noul("any")?,
        none,
    })
}

pub async fn rank(
    client: &Client,
    request: &str,
    items: &[String],
    prompts: &Prompts,
    finalist_text: Option<&(dyn Fn(usize) -> String + Sync)>,
) -> Result<Ranking, GreviError> {
    let all: Vec<(usize, String)> = items.iter().cloned().enumerate().collect();
    let size = client.backend().window();
    if all.len() <= size && finalist_text.is_none() {
        return window(client, request, &all, prompts).await;
    }
    let first = if all.len() <= size {
        vec![window(client, request, &all, prompts).await?]
    } else {
        futures::future::try_join_all(
            all.chunks(size)
                .map(|w| window(client, request, w, prompts)),
        )
        .await?
    };
    let mut pool: Vec<Candidate> = first
        .iter()
        .flat_map(|r| r.candidates.iter().take(PER_WINDOW_FINALISTS).copied())
        .collect();
    pool.sort_by(|a, b| b.p.total_cmp(&a.p));
    pool.truncate(MAX_FINALISTS);
    if pool.is_empty() {
        return Ok(Ranking {
            candidates: vec![],
            any: first.iter().map(|r| r.any).fold(0.0, f64::max),
            none: 1.0,
        });
    }
    let finals: Vec<(usize, String)> = pool
        .iter()
        .map(|c| {
            (
                c.index,
                finalist_text.map_or_else(|| items[c.index].clone(), |f| f(c.index)),
            )
        })
        .collect();
    window(client, request, &finals, prompts).await
}

/// Round 1 only: every window in parallel, the top `per_window` of each, best first.
/// For callers (such as `run`) whose second round is their own absolute question.
pub async fn shortlist(
    client: &Client,
    request: &str,
    items: &[String],
    prompts: &Prompts,
    per_window: usize,
) -> Result<Vec<Candidate>, GreviError> {
    let all: Vec<(usize, String)> = items.iter().cloned().enumerate().collect();
    let rounds = futures::future::try_join_all(
        all.chunks(client.backend().window())
            .map(|w| window(client, request, w, prompts)),
    )
    .await?;
    let mut pool: Vec<Candidate> = rounds
        .iter()
        .flat_map(|r| r.candidates.iter().take(per_window).copied())
        .collect();
    pool.sort_by(|a, b| b.p.total_cmp(&a.p));
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clip_cuts_on_char_boundaries() {
        assert_eq!(clip("héllo wörld", 3), "hé…");
        assert_eq!(clip("éé", 0), "");
        assert_eq!(clip("éé", 1), "…");
        assert_eq!(clip("short", 10), "short");
    }
}
