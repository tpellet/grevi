use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, HunchError};
use crate::jev::client::Client;
use crate::jev::{Question, Questions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const BATCH: usize = 10;

fn folders(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let hidden = e.file_name().to_string_lossy().starts_with('.');
            if p.is_dir() && !hidden {
                out.push(p.clone());
                if depth < 1 {
                    stack.push((p, depth + 1));
                }
            }
        }
    }
    out.sort();
    out.truncate(200);
    out
}

fn excerpt(p: &Path) -> String {
    use std::io::Read;
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Read at most 8 KiB: `read_to_string` would load a multi-GB video before failing UTF-8.
    let mut head = Vec::new();
    if let Ok(f) = std::fs::File::open(p) {
        let _ = f.take(8192).read_to_end(&mut head);
    }
    let text = match std::str::from_utf8(&head) {
        Ok(s) => Some(s),
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&head[..e.valid_up_to()]).ok(),
        Err(_) => None,
    };
    if let Some(s) = text.filter(|s| !s.contains('\0')) {
        return format!(
            "{name}: {}",
            crate::input::redact(&s.chars().take(2000).collect::<String>())
        );
    }
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        if let Ok(o) = std::process::Command::new("pdftotext")
            .args(["-l", "2"])
            .arg(p)
            .arg("-")
            .output()
        {
            if o.status.success() {
                return format!(
                    "{name}: {}",
                    crate::input::redact(
                        &String::from_utf8_lossy(&o.stdout)
                            .chars()
                            .take(2000)
                            .collect::<String>()
                    )
                );
            }
        }
    }
    name
}

fn undo(log: &Path) -> Result<Outcome, HunchError> {
    let text =
        std::fs::read_to_string(log).map_err(|e| HunchError::Input(format!("undo log: {e}")))?;
    let (mut restored, mut skipped) = (vec![], vec![]);
    for line in text.lines() {
        let Some((to, from)) = line.split_once('\t') else {
            continue;
        };
        if Path::new(from).exists() || !Path::new(to).exists() {
            skipped.push(
                serde_json::json!({ "file": to, "reason": "original path taken or file missing" }),
            );
        } else if std::fs::rename(to, from).is_ok() {
            restored.push(serde_json::json!({ "from": to, "to": from }));
        }
    }
    Ok(Outcome {
        exit: if restored.is_empty() {
            Exit::Abstain
        } else {
            Exit::Ok
        },
        human: format!("restored {} file(s)\n", restored.len()),
        data: serde_json::json!({ "moves": restored, "skipped": skipped, "undo_log": null, "applied": true }),
    })
}

pub async fn run(
    ctx: &Config,
    dir: &Path,
    into: Option<&Path>,
    apply: bool,
    undo_log: Option<&Path>,
) -> Result<Outcome, HunchError> {
    if let Some(l) = undo_log {
        return undo(l);
    }
    let root = into.unwrap_or(dir);
    let dests = folders(root);
    if dests.is_empty() {
        return Err(HunchError::Input(format!(
            "no folders under {} to sort into",
            root.display()
        )));
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| HunchError::Input(e.to_string()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    // Canonical order: `read_dir` is filesystem order, and option order moves an uncertain
    // probability (up to 0.23 measured); sorted, the batches and the cache key are stable.
    files.sort();
    if files.is_empty() {
        return Err(HunchError::EmptyInput("no files to sort"));
    }
    let client = Client::new(ctx)?;
    let folder_items: Vec<String> = dests
        .iter()
        .enumerate()
        .map(|(i, d)| format!("[D{i:03}] {}", d.strip_prefix(root).unwrap_or(d).display()))
        .collect();
    let mut crit: BTreeMap<String, Option<String>> = (0..dests.len())
        .map(|i| (format!("D{i:03}"), None))
        .collect();
    crit.insert(
        "NONE".into(),
        Some("no listed folder is a good home for this file".into()),
    );
    let jobs = files.chunks(BATCH).map(|chunk| {
        let state = serde_json::json!({ "files": chunk.iter().enumerate().map(|(k, f)| format!("[{k}] {}", excerpt(f))).collect::<Vec<_>>(), "folders": folder_items });
        let mut qs = Questions::new();
        for k in 0..chunk.len() {
            // As in `pick`: the Choice says which folder, the Noul says whether any folder fits at
            // all. Only the Noul is compared to the threshold (a Choice probability is relative to
            // its option set and does not share the Noul's calibration); the Choice must beat NONE.
            qs.insert(format!("f{k:02}"), Question::choice(format!("Which folder in `folders` is the right home for the file in `files[{k}]`? Choose NONE if none fits."), crit.clone()));
            qs.insert(format!("a{k:02}"), Question::noul_with(
                format!("Is one of the folders in `folders` the right home for the file in `files[{k}]`?"),
                "a listed folder is where this file belongs",
                "no listed folder is a good home for this file",
            ));
        }
        let client = &client;
        async move {
            let r = client.ask(&state, &qs).await?;
            (0..chunk.len()).map(|k| {
                let a = r.answers.get(&format!("f{k:02}")).cloned().unwrap_or_default();
                let c = a.choice.unwrap_or_else(|| "NONE".into());
                let probs = a.probabilities.unwrap_or_default();
                let p = probs.get(&c).copied().unwrap_or(0.0);
                let none = probs.get("NONE").copied().unwrap_or(0.0);
                Ok((c, p, none, r.noul(&format!("a{k:02}"))?))
            }).collect::<Result<Vec<_>, HunchError>>()
        }
    });
    let picks: Vec<(String, f64, f64, f64)> = futures::future::try_join_all(jobs)
        .await?
        .into_iter()
        .flatten()
        .collect();
    let (mut moves, mut skipped) = (vec![], vec![]);
    for (f, (c, p, none, any)) in files.iter().zip(&picks) {
        let Some(d) = c
            .strip_prefix('D')
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| dests.get(n))
        else {
            skipped.push(
                serde_json::json!({ "file": f.display().to_string(), "reason": "no folder fits" }),
            );
            continue;
        };
        let target = d.join(f.file_name().unwrap());
        if *any < ctx.threshold || *p <= *none {
            skipped.push(serde_json::json!({ "file": f.display().to_string(), "reason": format!("low confidence (any {any:.2}, folder {p:.2})") }));
        } else if target.exists() {
            skipped.push(
                serde_json::json!({ "file": f.display().to_string(), "reason": "target exists" }),
            );
        } else {
            moves.push((f.clone(), target, *p));
        }
    }
    let mut undo_path = None;
    if apply && !moves.is_empty() {
        let dir = ctx.cache_dir.clone().unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&dir).map_err(|e| HunchError::Input(e.to_string()))?;
        let log = dir.join(format!(
            "sort-undo-{}.tsv",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        ));
        let mut lines = String::new();
        for (from, to, _) in &moves {
            // Re-check right before the rename: `rename` would silently replace a file that
            // appeared since the dry run, and "never overwrites" must hold.
            if to.exists() {
                return Err(HunchError::Input(format!(
                    "{} appeared meanwhile; nothing overwritten, stopping (undo log: {})",
                    to.display(),
                    log.display()
                )));
            }
            std::fs::rename(from, to).map_err(|e| {
                // EXDEV: rename never copies across volumes.
                let hint = if e.raw_os_error() == Some(18) {
                    " (--into must be on the same volume as the files)"
                } else {
                    ""
                };
                HunchError::Input(format!("move {}: {e}{hint}", from.display()))
            })?;
            lines.push_str(&format!("{}\t{}\n", to.display(), from.display()));
            std::fs::write(&log, &lines).map_err(|e| HunchError::Input(e.to_string()))?;
        }
        undo_path = Some(log.display().to_string());
    }
    let human: String = moves
        .iter()
        .map(|(f, t, p)| format!("{:.2}  {} → {}\n", p, f.display(), t.display()))
        .collect();
    Ok(Outcome {
        exit: if moves.is_empty() {
            Exit::Abstain
        } else {
            Exit::Ok
        },
        data: serde_json::json!({ "moves": moves.iter().map(|(f, t, p)| serde_json::json!({ "from": f.display().to_string(), "to": t.display().to_string(), "p": p })).collect::<Vec<_>>(), "skipped": skipped, "undo_log": undo_path, "applied": apply }),
        human,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn undo_moves_back_only_when_the_original_path_is_free() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("Finance")).unwrap();
        let (a_from, a_to) = (d.path().join("a.txt"), d.path().join("Finance/a.txt"));
        let (b_from, b_to) = (d.path().join("b.txt"), d.path().join("Finance/b.txt"));
        std::fs::write(&a_to, "a").unwrap();
        std::fs::write(&b_to, "b").unwrap();
        std::fs::write(&b_from, "taken").unwrap();
        let log = d.path().join("undo.tsv");
        std::fs::write(
            &log,
            format!(
                "{}\t{}\n{}\t{}\nnot a log line\n",
                a_to.display(),
                a_from.display(),
                b_to.display(),
                b_from.display()
            ),
        )
        .unwrap();
        let out = undo(&log).unwrap();
        assert_eq!(out.exit, Exit::Ok);
        assert!(a_from.exists() && !a_to.exists());
        assert!(b_to.exists(), "a taken original path is never overwritten");
        assert_eq!(std::fs::read_to_string(&b_from).unwrap(), "taken");
        assert_eq!(out.data["skipped"].as_array().unwrap().len(), 1);
        // Nothing left to restore: exit 3, as documented.
        assert_eq!(undo(&log).unwrap().exit, Exit::Abstain);
        assert_eq!(
            undo(Path::new("/nonexistent/undo.tsv"))
                .err()
                .unwrap()
                .exit(),
            Exit::Input
        );
    }
}
