use crate::{
    cmd::{Exec, Outcome},
    config::Config,
    exit::{AMBIGUOUS, Exit, INSUFFICIENT_EVIDENCE, JevifyError, NO_MATCH, UNSURE_FLAG},
    input, jev,
    jev::{Question, Questions, client::Client},
    marker::{self, Marker},
    output,
    records::{Record, Split},
    source::{self, Scope},
    tournament::{self, Decision, Finalists, Prompts, Ranking, Shortlist},
};
use futures::future::join_all;
use serde_json::json;
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io::{IsTerminal, Read},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
};

const FLAG_BAND: f64 = 0.15;
const MAX_CONTEXT_CHARS: usize = 96_000;
const MAX_FINALISTS: usize = 24;

pub struct FillFlags {
    pub dry_run: bool,
    pub quiet: bool,
    pub candidates: Option<PathBuf>,
    pub context: Option<PathBuf>,
    pub field: Option<usize>,
    pub key: Option<String>,
    pub split: Split,
}

pub async fn run(
    ctx: &Config,
    flags: FillFlags,
    cmd: &[OsString],
    machine: bool,
) -> Result<Outcome, JevifyError> {
    if machine && !flags.dry_run {
        return Err(JevifyError::Usage(
            "machine output requires --dry-run".into(),
        ));
    }
    let args = marker::parse(cmd).map_err(|e| JevifyError::Usage(e.to_string()))?;
    let markers: Vec<_> = args.iter().flat_map(|arg| &arg.markers).collect();
    let env = source::Env::from_process(source::LISTER_TIMEOUT);
    // One `Kind` per marker (`None` for `one` and `flag`), resolved once, before any other I/O.
    let mut kinds = Vec::with_capacity(markers.len());
    for marker in &markers {
        kinds.push(validate_kind(
            marker,
            &args[marker.argv_index].literal,
            &env,
        )?);
        if marker.options.len() > ctx.backend.window() {
            return Err(JevifyError::Usage(format!(
                "one has {} options; the backend accepts at most {}",
                marker.options.len(),
                ctx.backend.window()
            )));
        }
    }
    marker::check_stdin_roles(&args, flags.candidates.is_some(), flags.context.is_some())
        .map_err(|e| JevifyError::Usage(e.to_string()))?;
    if flags.field == Some(0) || flags.field.is_some() && flags.key.is_some() {
        return Err(JevifyError::Usage(
            "use a positive --field or --key, not both".into(),
        ));
    }
    let program = cmd
        .first()
        .ok_or_else(|| JevifyError::Usage("missing command".into()))?
        .clone();
    tokio::task::spawn_blocking(move || check_program(&program))
        .await
        .map_err(|e| JevifyError::Input(e.to_string()))??;
    let needs_candidates = markers.iter().any(|m| m.kind == "-");
    let needs_context = markers
        .iter()
        .any(|m| matches!(m.kind.as_str(), "one" | "flag"));
    let stdin_null =
        needs_candidates && flags.candidates.is_none() || needs_context && flags.context.is_none();
    let candidates = if needs_candidates {
        read_input(flags.candidates.clone()).await?
    } else {
        vec![]
    };
    let context = if needs_context {
        read_input(flags.context.clone()).await?
    } else {
        vec![]
    };
    let context = input::redact(&input::split_lines(&String::from_utf8_lossy(&context)).join("\n"));
    let insufficient =
        context.chars().count() > MAX_CONTEXT_CHARS.min(ctx.backend.max_state_chars());
    let limit = ctx.backend.window() * (ctx.backend.window() / 3);
    // The literal before a path marker is its scope; every other kind lists from the cwd.
    let prefix_of = |i: usize| -> Option<PathBuf> {
        kinds[i].as_ref().filter(|k| k.path_kind).and_then(|_| {
            (!markers[i].prefix.is_empty()).then(|| PathBuf::from(&markers[i].prefix))
        })
    };
    let mut scopes = Vec::new();
    for (i, m) in markers.iter().enumerate() {
        if kinds[i].is_some() {
            let key = (m.kind.clone(), prefix_of(i));
            if !scopes.contains(&key) {
                scopes.push(key);
            }
        }
    }
    let listings = join_all(scopes.iter().map(|(kind, prefix)| {
        let scope = if kind == "-" {
            Scope::Input {
                bytes: candidates.clone(),
                split: flags.split,
                field: flags.field,
                key: flags.key.clone(),
            }
        } else {
            Scope::Prefix(prefix.clone())
        };
        source::enumerate(kind, scope, limit, &env)
    }))
    .await;
    let mut states = Vec::new();
    for (i, m) in markers.iter().enumerate() {
        let mut state = State::default();
        if let Some(kind) = &kinds[i] {
            let prefix = prefix_of(i);
            let index = scopes
                .iter()
                .position(|(kind, scope)| kind == &m.kind && scope == &prefix)
                .unwrap();
            let listing = match &listings[index] {
                Ok(listing) => listing,
                // Move the first error in marker order, without requiring errors to be Clone.
                Err(_) => return Err(listings.into_iter().nth(index).unwrap().unwrap_err()),
            };
            state.records = listing.records.clone();
            state.omitted = listing.omitted;
            state.total = listing.total;
            state.newest = listing.ordered && listing.total > listing.records.len();
            if m.opens_argument && !kind.path_kind {
                state.records.retain(|r| {
                    let keep = !r.handle.as_bytes().starts_with(b"-");
                    state.omitted += usize::from(!keep);
                    keep
                });
            }
            if state.records.len() > limit && !listing.ordered {
                return Err(JevifyError::Kinded {
                    kind: "too_many",
                    exit: Exit::Input,
                    message: format!(
                        "{} candidates exceed the fill capacity of {limit}",
                        state.records.len()
                    ),
                    hint: "narrow with a literal path prefix, or pipe a narrower list into '@{-:description}'",
                    example: "grep pattern candidates | jevify fill -- command '@{-:description}'",
                });
            }
            if state.records.is_empty() {
                state.reason = Some(NO_MATCH);
            }
        } else if insufficient {
            state.reason = Some(INSUFFICIENT_EVIDENCE);
        }
        states.push(state);
    }
    // An oversized context cannot authorize any request on a partial input.
    if insufficient {
        for state in &mut states {
            state.reason.get_or_insert(INSUFFICIENT_EVIDENCE);
        }
        return finish(ctx, &flags, &args, &markers, states, stdin_null, machine);
    }
    let client = Client::new(ctx)?;
    let prompts = Prompts {
        choose: "Which item best satisfies request? Select NONE when no item fits.".into(),
        none: "No item satisfies the request".into(),
        any: "Does any item satisfy request?".into(),
    };
    let items: Vec<Vec<String>> = states
        .iter()
        .map(|s| s.records.iter().map(|r| r.evidence.clone()).collect())
        .collect();
    let mut questions = Questions::new();
    for (i, m) in markers.iter().enumerate() {
        if m.kind == "one" {
            let mut criteria: BTreeMap<_, _> = m
                .options
                .iter()
                .enumerate()
                .map(|(i, option)| (format!("L{i:03}"), Some(option.clone())))
                .collect();
            criteria.insert("NONE".into(), Some("None of these options fits".into()));
            questions.insert(format!("m{i}"), Question::choice(&m.description, criteria));
        } else if m.kind == "flag" {
            questions.insert(format!("m{i}"), Question::noul(&m.description));
        }
    }
    let context_state = json!(context);
    let listing_round = join_all(markers.iter().enumerate().map(|(i, m)| {
        let client = &client;
        let prompts = &prompts;
        let texts = &items[i];
        async move {
            if texts.is_empty() {
                Ok(None)
            } else {
                tournament::shortlist(client, &m.description, texts, prompts, Finalists::ThreeOnly)
                    .await
                    .map(Some)
            }
        }
    }));
    let questions: Vec<_> = questions.into_iter().collect();
    let batches: Vec<Questions> = questions
        .chunks(20)
        .map(|chunk| chunk.iter().cloned().collect())
        .collect();
    let context_round = join_all(
        batches
            .iter()
            .map(|batch| client.ask(&context_state, batch)),
    );
    let (shortlists, answers) = futures::join!(listing_round, context_round);
    let mut errors = Vec::new();
    let mut shortlists: Vec<Option<Shortlist>> = shortlists
        .into_iter()
        .enumerate()
        .map(|(i, result)| match result {
            Ok(value) => value,
            Err(error) => {
                errors.push((i, error));
                None
            }
        })
        .collect();
    let mut merged: Option<jev::Response> = None;
    for (batch, result) in batches.iter().zip(answers) {
        match result {
            Ok(answer) => {
                if let Some(merged) = &mut merged {
                    merged.answers.extend(answer.answers);
                } else {
                    merged = Some(answer);
                }
            }
            Err(error) => {
                let index = batch
                    .keys()
                    .filter_map(|key| key.strip_prefix('m')?.parse::<usize>().ok())
                    .min()
                    .unwrap();
                errors.push((index, error));
            }
        }
    }
    if let Some((_, error)) = errors.into_iter().min_by_key(|(index, _)| *index) {
        return Err(error);
    }
    let answers = merged;
    if answers.is_some() || shortlists.iter().any(Option::is_some) {
        guard_model(ctx)?;
    }
    for (i, m) in markers.iter().enumerate() {
        if let Some(answer) = &answers {
            if m.kind == "flag" {
                let p = answer.noul(&format!("m{i}"))?;
                let (exit, verdict) = super::is::band_verdict(p, ctx.threshold, FLAG_BAND);
                states[i].p = Some(p);
                states[i].detail = format!(
                    "{}: {verdict} {p:.2}{}",
                    m.flag.as_deref().unwrap(),
                    if exit == Exit::No { ", left out" } else { "" }
                );
                if exit == Exit::Abstain {
                    states[i].reason = Some(UNSURE_FLAG);
                    states[i].detail.push_str(&format!(
                        "; write {} or drop the marker",
                        m.flag.as_deref().unwrap()
                    ));
                } else {
                    states[i].handle = Some(if exit == Exit::Ok {
                        m.flag.as_deref().unwrap().into()
                    } else {
                        OsString::new()
                    });
                }
            } else if m.kind == "one" {
                let probabilities = answer.probs(&format!("m{i}"))?;
                states[i].records = m
                    .options
                    .iter()
                    .map(|o| Record {
                        handle: o.into(),
                        evidence: o.clone(),
                        raw: 0..0,
                    })
                    .collect();
                states[i].total = m.options.len();
                let mut candidates: Vec<_> = m
                    .options
                    .iter()
                    .enumerate()
                    .map(|(index, _)| tournament::Candidate {
                        index,
                        p: probabilities[&format!("L{index:03}")],
                    })
                    .collect();
                candidates.sort_by(|a, b| b.p.total_cmp(&a.p));
                apply_ranking(
                    &mut states[i],
                    &Ranking {
                        candidates,
                        any: 1.0,
                        none: probabilities["NONE"],
                        windows: 1,
                        n: 3,
                    },
                    ctx.threshold,
                );
            }
        }
    }
    let finals = join_all(markers.iter().enumerate().map(|(i, m)| {
        let first = shortlists[i].take();
        let records = &states[i].records;
        let client = &client;
        let prompts = &prompts;
        let env = &env;
        let tier_two = kinds[i].as_ref().is_some_and(|k| k.has_tier_two);
        let prefix = prefix_of(i).unwrap_or_default();
        async move {
            let Some(first) = first else {
                return Ok(None);
            };
            if first.windows.len() == 1 {
                let ranking = &first.windows[0];
                if !tier_two
                    || !matches!(
                        tournament::decide(ranking, ctx.threshold),
                        Decision::Ambiguous(_)
                    )
                {
                    return Ok(Some((ranking.clone(), 0)));
                }
            }
            let handles: Vec<_> = first
                .finalists
                .iter()
                .take(MAX_FINALISTS)
                .map(|c| records[c.index].handle.clone())
                .collect();
            let (evidence, withheld) = if tier_two {
                source::enrich_in(&m.kind, &prefix, &handles, env).await
            } else {
                (vec![], 0)
            };
            let items: Vec<_> = first
                .finalists
                .iter()
                .enumerate()
                .map(|(rank, c)| {
                    let mut text = records[c.index].evidence.clone();
                    if let Some(extra) = evidence.get(rank) {
                        text.push('\n');
                        text.push_str(extra);
                    }
                    (c.index, text)
                })
                .collect();
            let mut ranking = tournament::window(client, &m.description, &items, prompts).await?;
            ranking.windows = first.windows.len();
            Ok::<_, JevifyError>(Some((ranking, withheld)))
        }
    }))
    .await;
    for (i, ranking) in finals.into_iter().enumerate() {
        if let Some((ranking, withheld)) = ranking? {
            states[i].withheld = withheld;
            apply_ranking(&mut states[i], &ranking, ctx.threshold);
        }
    }
    if answers.is_some() || items.iter().any(|items| !items.is_empty()) {
        guard_model(ctx)?;
    }
    finish(ctx, &flags, &args, &markers, states, stdin_null, machine)
}

#[derive(Default)]
struct State {
    records: Vec<Record>,
    total: usize,
    omitted: usize,
    /// An ordered listing above the limit kept its newest part.
    newest: bool,
    /// Finalists whose excerpt the withholding policy kept out of round two.
    withheld: usize,
    handle: Option<OsString>,
    reason: Option<&'static str>,
    p: Option<f64>,
    detail: String,
}

impl State {
    /// `candidates N`, `candidates N of M` when the listing was cut, `newest first` when the
    /// cut kept the head of an ordered listing, and the omitted count when there is one.
    fn count_line(&self) -> String {
        let mut line = format!("candidates {}", self.records.len());
        if self.total > self.records.len() {
            line.push_str(&format!(" of {}", self.total));
            if self.newest {
                line.push_str(", newest first");
            }
        }
        if self.omitted > 0 {
            line.push_str(&format!(", omitted {}", self.omitted));
        }
        line
    }
}

fn guard_model(ctx: &Config) -> Result<(), JevifyError> {
    let model = ctx.meta().model.unwrap_or_else(|| "unknown".into());
    if !jev::all_jev(&model) {
        return Err(JevifyError::Unavailable(format!(
            "answered by {model}, not Jev"
        )));
    }
    Ok(())
}

fn apply_ranking(state: &mut State, ranking: &Ranking, threshold: f64) {
    match tournament::decide(ranking, threshold) {
        Decision::Found(best) => {
            let record = &state.records[best.index];
            let duplicates = state
                .records
                .iter()
                .filter(|r| r.evidence == record.evidence)
                .count();
            if duplicates > 1 {
                state.reason = Some(AMBIGUOUS);
                state.detail = format!("{duplicates} candidates share the same evidence");
                return;
            }
            state.handle = Some(record.handle.clone());
            state.p = Some(best.p);
            state.detail = format!(
                "{} {:.2} (next {:.2}, none {:.2}) {}; {}, windows {}{}",
                record.handle.to_string_lossy(),
                best.p,
                ranking.candidates.get(1).map_or(0.0, |c| c.p),
                ranking.none,
                record.evidence,
                state.count_line(),
                ranking.windows,
                if state.withheld > 0 {
                    format!(", excerpts withheld: {}", state.withheld)
                } else {
                    String::new()
                }
            );
        }
        Decision::NoMatch => state.reason = Some(NO_MATCH),
        Decision::Ambiguous(closest) => {
            state.reason = Some(AMBIGUOUS);
            state.detail = format!(
                "closest: {}",
                closest
                    .iter()
                    .map(|c| format!(
                        "{} ({:.2})",
                        state.records[c.index].handle.to_string_lossy(),
                        c.p
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

fn finish(
    ctx: &Config,
    flags: &FillFlags,
    args: &[marker::Arg],
    markers: &[&Marker],
    states: Vec<State>,
    stdin_null: bool,
    machine: bool,
) -> Result<Outcome, JevifyError> {
    let reason = states.iter().find_map(|s| s.reason);
    let model = ctx.meta().model.unwrap_or_else(|| "not requested".into());
    for (m, state) in markers.iter().zip(&states) {
        if state.reason.is_none() && !flags.quiet {
            eprintln!(
                "jevify fill: {} {}; model {}",
                m.kind,
                output::status_escape(&state.detail),
                output::status_escape(&model)
            );
        }
    }
    for (m, state) in markers.iter().zip(&states) {
        if let Some(reason) = state.reason {
            eprintln!(
                "jevify fill: not run: arg {} {}: {reason}; {}; candidates {} of {}, omitted {}; model {}",
                m.argv_index + 1,
                m.kind,
                output::status_escape(&state.detail),
                state.records.len(),
                state.total,
                state.omitted,
                output::status_escape(&model)
            );
        }
    }
    let mut data = json!({"reason": reason, "markers": markers.iter().zip(&states).map(|(m,s)| json!({
        "arg": m.argv_index + 1, "kind": m.kind, "reason": s.reason,
        "handle": s.handle.as_ref().map(|h| h.to_string_lossy()), "p": s.p,
        "candidates": s.records.len(), "total": s.total, "omitted": s.omitted,
    })).collect::<Vec<_>>()});
    if reason.is_some() {
        return Ok(Outcome {
            exit: Exit::Abstain,
            data,
            human: vec![],
            exec: None,
        });
    }
    let handles: Vec<_> = states
        .into_iter()
        .map(|s| s.handle.expect("every marker resolved"))
        .collect();
    let argv = marker::substitute(args, &handles).map_err(|e| JevifyError::Usage(e.to_string()))?;
    if machine && argv.iter().any(|arg| arg.to_str().is_none()) {
        return Err(JevifyError::cannot_run(
            "machine output cannot represent non-UTF-8 argv".into(),
        ));
    }
    data["argv"] = json!(argv.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>());
    let mut quoted = output::shell_quote(&argv);
    if !flags.quiet {
        eprintln!(
            "jevify fill: exec {}",
            output::status_escape(&String::from_utf8_lossy(&quoted))
        );
    }
    if flags.dry_run {
        quoted.push(b'\n');
        Ok(Outcome {
            exit: Exit::Ok,
            data,
            human: quoted,
            exec: None,
        })
    } else {
        Ok(Outcome {
            exit: Exit::Ok,
            data,
            human: vec![],
            exec: Some(Exec { argv, stdin_null }),
        })
    }
}

fn check_stdin_terminal(is_terminal: bool) -> Result<(), JevifyError> {
    if is_terminal {
        Err(JevifyError::stdin_is_tty(
            "pipe candidates or context into fill, or supply a file".into(),
        ))
    } else {
        Ok(())
    }
}

async fn read_input(path: Option<PathBuf>) -> Result<Vec<u8>, JevifyError> {
    tokio::task::spawn_blocking(move || {
        let Some(path) = path else {
            check_stdin_terminal(std::io::stdin().is_terminal())?;
            return match input::read_stdin_bytes() {
                Err(JevifyError::EmptyInput(_)) => Ok(vec![]),
                result => result,
            };
        };
        let file = std::fs::File::open(&path)
            .map_err(|e| JevifyError::Input(format!("{}: {e}", path.display())))?;
        let mut bytes = vec![];
        file.take(input::MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| JevifyError::Input(e.to_string()))?;
        if bytes.len() > input::MAX_BYTES {
            return Err(JevifyError::InputTooLarge("input exceeds 64 MiB".into()));
        }
        Ok(bytes)
    })
    .await
    .map_err(|e| JevifyError::Input(e.to_string()))?
}

fn check_program(program: &OsStr) -> Result<(), JevifyError> {
    let executable = |path: &Path| {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    let found = if program.as_bytes().contains(&b'/') {
        executable(Path::new(program))
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .any(|dir| executable(&dir.join(program)))
    };
    if found && !program.as_bytes().contains(&0) {
        Ok(())
    } else {
        Err(JevifyError::cannot_run(format!(
            "{} is not an executable on PATH",
            program.to_string_lossy()
        )))
    }
}

/// The kind of a listing marker; `None` for `one` and `flag`. A name that is neither coded nor
/// shipped is looked up in the user's recipes; a name found nowhere is exit 2 with the nearest
/// kind, and a bad recipe file is `recipe_invalid` with its line number.
fn validate_kind(
    marker: &Marker,
    argument: &OsStr,
    env: &source::Env,
) -> Result<Option<source::Kind>, JevifyError> {
    if matches!(marker.kind.as_str(), "one" | "flag") {
        return Ok(None);
    }
    if let Some(kind) = source::lookup(&marker.kind, env)? {
        return Ok(Some(kind));
    }
    let user: Vec<String> = source::catalog(env)
        .kinds
        .into_iter()
        .filter(|entry| entry.origin == "user")
        .map(|entry| entry.name)
        .collect();
    let kinds: Vec<&str> = source::KINDS
        .iter()
        .copied()
        .chain(["one", "flag"])
        .chain(user.iter().map(String::as_str))
        .collect();
    let nearest = kinds
        .iter()
        .min_by_key(|kind| edit_distance(&marker.kind, kind))
        .unwrap();
    let mut literal = argument.as_bytes().to_vec();
    literal.insert(marker.span.start, b'@');
    let literal = output::status_escape(&String::from_utf8_lossy(&literal)).replace('\'', "'\\''");
    Err(JevifyError::Usage(format!(
        "unknown kind '{}'; nearest kind: {nearest}; kinds: {}; for a literal write '{literal}'",
        marker.kind,
        kinds.join(", ")
    )))
}

fn edit_distance(a: &str, b: &str) -> usize {
    let mut row: Vec<_> = (0..=b.len()).collect();
    for (i, left) in a.bytes().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right) in b.bytes().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(left != right))
                .min(above + 1)
                .min(row[j] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_requires_explicit_input() {
        assert!(check_stdin_terminal(false).is_ok());
        let error = check_stdin_terminal(true).unwrap_err();
        assert_eq!(error.exit(), Exit::Input);
        assert_eq!(error.kind(), "stdin_is_tty");
    }

    #[test]
    fn nearest_kind_uses_edit_distance() {
        assert_eq!(edit_distance("brnch", "branch"), 1);
        assert_eq!(edit_distance("", "branch"), 6);
        assert_eq!(edit_distance("branch", "branch"), 0);
    }

    #[test]
    fn ranking_requires_fit_ratio_and_distinct_evidence() {
        for (best, second, none, any, expected) in [
            (0.9, 0.05, 0.05, 0.9, None),
            (0.45, 0.45, 0.1, 0.9, Some(AMBIGUOUS)),
            (0.5, 0.1, 0.4, 0.9, Some(AMBIGUOUS)),
            (0.9, 0.05, 0.05, 0.1, Some(NO_MATCH)),
            (0.1, 0.1, 0.8, 0.9, Some(NO_MATCH)),
        ] {
            let mut state = State {
                records: vec![
                    Record {
                        handle: "a".into(),
                        evidence: "first".into(),
                        raw: 0..0,
                    },
                    Record {
                        handle: "b".into(),
                        evidence: "second".into(),
                        raw: 0..0,
                    },
                ],
                total: 2,
                ..State::default()
            };
            let ranking = Ranking {
                candidates: vec![
                    tournament::Candidate { index: 0, p: best },
                    tournament::Candidate {
                        index: 1,
                        p: second,
                    },
                ],
                any,
                none,
                windows: 1,
                n: 3,
            };
            apply_ranking(&mut state, &ranking, 0.5);
            assert_eq!(state.reason, expected);
            assert_eq!(state.handle.is_some(), expected.is_none());
            if expected.is_none() {
                state.handle = None;
                state.records[1].evidence = "first".into();
                apply_ranking(&mut state, &ranking, 0.5);
                assert_eq!(state.reason, Some(AMBIGUOUS));
                assert!(state.handle.is_none());
                assert_eq!(state.detail, "2 candidates share the same evidence");
            }
        }
        let mut empty = State::default();
        apply_ranking(
            &mut empty,
            &Ranking {
                candidates: vec![],
                any: 1.0,
                none: 0.0,
                windows: 0,
                n: 3,
            },
            0.5,
        );
        assert_eq!(empty.reason, Some(NO_MATCH));
    }

    #[test]
    fn program_validation_rejects_missing_nonexecutables_and_nul() {
        assert!(check_program(OsStr::new("/bin/sh")).is_ok());
        for program in ["", "/", "/not/a/jevify/program", "/bin/sh\0"] {
            let error = check_program(OsStr::new(program)).unwrap_err();
            assert_eq!(error.exit(), Exit::Input);
            assert_eq!(error.kind(), "cannot_run");
        }
    }
}
