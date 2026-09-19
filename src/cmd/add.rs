use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, HunchError};
use crate::gitdiff;
use crate::jev::client::Client;
use crate::jev::{Question, Questions};
use std::io::Write;
use std::process::{Command, Stdio};

const BATCH: usize = 20;
/// Hunk text sent in the API state (never the patch that gets staged) is clipped so 20 hunks of
/// lockfile or generated code cannot push one request past the 32k-token limit.
const HUNK_CHARS: usize = 3_000;

pub async fn run(
    ctx: &Config,
    topic: &str,
    yes: bool,
    dry_run: bool,
    machine: bool,
) -> Result<Outcome, HunchError> {
    // From a subdirectory `git diff` lists the whole repo, but `git apply` silently skips paths
    // outside the cwd (exit 0): run both at the top level, or hunks are reported staged but are not.
    let top = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| HunchError::Input(format!("git: {e}")))?;
    if !top.status.success() {
        return Err(HunchError::Input(
            "not a git repository (or git failed)".into(),
        ));
    }
    let top = std::path::PathBuf::from(String::from_utf8_lossy(&top.stdout).trim());
    // Explicit prefixes: a user's `diff.noprefix=true` would otherwise break file_name and git apply.
    let out = Command::new("git")
        .current_dir(&top)
        .args([
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "-U3",
        ])
        .output()
        .map_err(|e| HunchError::Input(format!("git: {e}")))?;
    if !out.status.success() {
        return Err(HunchError::Input(
            "not a git repository (or git failed)".into(),
        ));
    }
    let files = gitdiff::parse(&String::from_utf8_lossy(&out.stdout));
    let flat: Vec<(usize, usize, String)> = files
        .iter()
        .enumerate()
        .flat_map(|(fi, f)| {
            f.hunks.iter().enumerate().map(move |(hi, h)| {
                (
                    fi,
                    hi,
                    crate::tournament::clip(
                        &crate::input::redact(&format!("{}{}", h.header, h.body)),
                        HUNK_CHARS,
                    ),
                )
            })
        })
        .collect();
    if flat.is_empty() {
        return Err(HunchError::EmptyInput(
            "no unstaged changes to tracked files (untracked files are never staged by add)",
        ));
    }
    let client = Client::new(ctx)?;
    let file_name = |fi: usize| {
        files[fi]
            .header
            .lines()
            .next()
            .unwrap_or_default()
            .rsplit(" b/")
            .next()
            .unwrap_or_default()
            .to_string()
    };
    let batches = flat.chunks(BATCH).map(|chunk| {
        let state = serde_json::json!({ "topic": topic, "hunks": chunk.iter().map(|(fi, _, h)| format!("file {}\n{}", file_name(*fi), h)).collect::<Vec<_>>() });
        let mut qs = Questions::new();
        for i in 0..chunk.len() {
            qs.insert(format!("h{i:02}"), Question::noul_with(
                format!("Is the change in `hunks[{i}]` part of the work described by `topic`?"),
                "this hunk implements or directly supports the described work",
                "this hunk is about something else",
            ));
        }
        let client = &client;
        async move {
            let r = client.ask(&state, &qs).await?;
            (0..chunk.len()).map(|i| r.noul(&format!("h{i:02}"))).collect::<Result<Vec<f64>, HunchError>>()
        }
    });
    let ps: Vec<f64> = futures::future::try_join_all(batches)
        .await?
        .into_iter()
        .flatten()
        .collect();
    let chosen: Vec<bool> = ps.iter().map(|p| *p >= ctx.threshold).collect();
    let rows: Vec<_> = flat.iter().zip(&ps).zip(&chosen)
        .map(|(((fi, hi, _), p), c)| serde_json::json!({ "file": file_name(*fi), "header": files[*fi].hunks[*hi].header.trim(), "p": p, "staged": *c && !dry_run }))
        .collect();
    let n = chosen.iter().filter(|c| **c).count();
    if n == 0 {
        return Ok(Outcome {
            exit: Exit::Abstain,
            data: serde_json::json!({ "hunks": rows }),
            human: String::new(),
        });
    }
    let mut summary = String::new();
    for (((fi, hi, _), p), c) in flat.iter().zip(&ps).zip(&chosen) {
        summary.push_str(&format!(
            "{} {:.2} {} {}\n",
            if *c { "+" } else { " " },
            p,
            file_name(*fi),
            files[*fi].hunks[*hi].header.trim()
        ));
    }
    if dry_run {
        return Ok(Outcome {
            exit: Exit::Ok,
            data: serde_json::json!({ "hunks": rows }),
            human: summary,
        });
    }
    // Machine mode requires --yes; humans confirm on /dev/tty (no TTY → declined, nothing staged).
    if !yes
        && (machine
            || !matches!(
                crate::cmd::confirm_tty(&format!("{summary}Stage {n} hunk(s)? [y/N] "))?,
                Some(true)
            ))
    {
        return Err(HunchError::Declined);
    }
    let keep = |fi: usize, hi: usize| {
        flat.iter()
            .zip(&chosen)
            .any(|((f, h, _), c)| *c && *f == fi && *h == hi)
    };
    let p = gitdiff::patch(&files, &keep);
    let mut child = Command::new("git")
        .current_dir(&top)
        .args(["apply", "--cached", "--recount", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| HunchError::Input(e.to_string()))?;
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(p.as_bytes())
        .map_err(|e| HunchError::Input(e.to_string()))?;
    if !child
        .wait()
        .map_err(|e| HunchError::Input(e.to_string()))?
        .success()
    {
        return Err(HunchError::Input(
            "git apply --cached rejected the patch; nothing was staged".into(),
        ));
    }
    Ok(Outcome {
        exit: Exit::Ok,
        data: serde_json::json!({ "hunks": rows }),
        human: summary,
    })
}
