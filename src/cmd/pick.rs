use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, GreviError};
use crate::jev::client::Client;
use crate::tournament::{Prompts, rank};
use std::path::{Path, PathBuf};

/// 100k lines would be 500 requests: a 429 storm and ~25 s. pick ranks a list, it does not scan.
pub const MAX_LINES: usize = 20_000;

pub async fn run(
    ctx: &Config,
    intent: &str,
    top: usize,
    index: bool,
    files: Option<&Path>,
) -> Result<Outcome, GreviError> {
    if top == 0 {
        return Err(GreviError::Usage("-n must be at least 1".into()));
    }
    if files.is_some() && index {
        return Err(GreviError::Usage(
            "--index numbers stdin lines; with --files the match is a path".into(),
        ));
    }
    let client = Client::new(ctx)?;
    // With --files the candidates are paths under DIR and stdin is not read.
    let root = files.map(canonical_dir).transpose()?;
    let lines = match &root {
        Some(root) => {
            let root = root.clone();
            tokio::task::spawn_blocking(move || list_files(&root))
                .await
                .map_err(|e| GreviError::Input(e.to_string()))?
        }
        None => crate::input::read_stdin_async().await?,
    };
    if root.is_some() && lines.is_empty() {
        return Err(GreviError::EmptyInput(
            "no regular files under the directory (hidden and ignored files are skipped)",
        ));
    }
    // Blank lines never leave the machine: as empty items they would take window slots and tokens.
    // A repeated line is sent once (the first occurrence keeps its line number): two identical
    // items split the Choice mass between them.
    let mut seen = std::collections::HashSet::new();
    let kept: Vec<usize> = (0..lines.len())
        .filter(|&i| !lines[i].trim().is_empty() && seen.insert(lines[i].trim()))
        .collect();
    if kept.len() > MAX_LINES {
        return Err(GreviError::InputTooLarge(if root.is_some() {
            format!("more than {MAX_LINES} files; choose a narrower directory")
        } else {
            format!("more than {MAX_LINES} lines; filter first (rg, head) or split the list")
        }));
    }
    let items: Vec<String> = kept.iter().map(|&i| lines[i].clone()).collect();
    let prompts = if root.is_some() {
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
        let excerpt = root
            .as_ref()
            .map(|r| crate::cmd::sort::excerpt(&r.join(&items[k])))
            .unwrap_or_default();
        format!("{}\n{excerpt}", items[k])
    };
    let finalist_text: Option<&(dyn Fn(usize) -> String + Sync)> =
        root.as_ref().map(|_| &with_content as _);
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
                let text = match files {
                    Some(dir) => shown(dir, &lines[i]),
                    None => lines[i].clone(),
                };
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
        data: serde_json::json!({ "matches": matches, "any": ranking.any, "source": if root.is_some() { "files" } else { "stdin" } }),
        human,
    })
}

fn canonical_dir(dir: &Path) -> Result<PathBuf, GreviError> {
    // The printed path is built from DIR as typed, so DIR must survive as text.
    if dir.to_str().is_none() {
        return Err(GreviError::Usage("--files DIR must be valid UTF-8".into()));
    }
    let root = std::fs::canonicalize(dir)
        .map_err(|e| GreviError::Input(format!("--files {}: {e}", dir.display())))?;
    if !root.is_dir() {
        return Err(GreviError::Input(format!(
            "--files {}: not a directory",
            dir.display()
        )));
    }
    Ok(root)
}

/// No path component starts with a dot: `.env`, `.git/`, `.ssh/` never become candidates.
fn visible(rel: &str) -> bool {
    !rel.split('/').any(|c| c.starts_with('.'))
}

/// Regular files under `root`, relative to it, sorted: option order is part of the request.
/// In a git work tree the list comes from git, so `.gitignore` applies; elsewhere from a walk.
/// ponytail: names that are not UTF-8 are skipped, since a lossy name is not a usable handle.
fn list_files(root: &Path) -> Vec<String> {
    let git = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-co", "--exclude-standard", "-z"])
        .output();
    let mut paths: Vec<String> = match git {
        Ok(o) if o.status.success() => o
            .stdout
            .split(|b| *b == 0)
            .filter_map(|s| String::from_utf8(s.to_vec()).ok())
            .filter(|s| !s.is_empty() && visible(s))
            // git also lists tracked symlinks and tracked files deleted from the work tree.
            .filter(|s| root.join(s).symlink_metadata().is_ok_and(|m| m.is_file()))
            .take(MAX_LINES + 1)
            .collect(),
        _ => walk(root),
    };
    paths.sort();
    paths.dedup();
    paths
}

/// Never follows a symlink and never enters a hidden directory. Stops one past the cap: the
/// caller rejects the input, so the rest of a huge tree is not worth reading.
fn walk(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&rel)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|n| !n.starts_with('.')) else {
                continue;
            };
            let child = rel.join(name);
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(child),
                Ok(t) if t.is_file() => out.extend(child.to_str().map(String::from)),
                _ => (),
            }
            if out.len() > MAX_LINES {
                return out;
            }
        }
    }
    out
}

/// The match as a path usable from the current directory. A leading dash gets `./`, so the
/// path can never be read as an option by the command that receives it.
fn shown(dir: &Path, rel: &str) -> String {
    let path = if dir == Path::new(".") {
        PathBuf::from(rel)
    } else {
        dir.join(rel)
    };
    let s = path.to_string_lossy().into_owned();
    if s.starts_with('-') {
        format!("./{s}")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_components_are_not_candidates() {
        assert!(visible("src/cmd/pick.rs"));
        assert!(visible("notes.v2.txt"));
        assert!(!visible(".env"));
        assert!(!visible("config/.secrets/key.txt"));
        assert!(!visible(".git/config"));
    }

    #[test]
    fn shown_paths_work_from_the_current_directory_and_never_look_like_options() {
        assert_eq!(shown(Path::new("."), "src/a.rs"), "src/a.rs");
        assert_eq!(
            shown(Path::new("docs"), "guide/faq.md"),
            "docs/guide/faq.md"
        );
        assert_eq!(shown(Path::new("."), "-rf"), "./-rf");
        assert_eq!(shown(Path::new("-dir"), "a"), "./-dir/a");
    }

    #[test]
    fn walk_skips_hidden_entries_and_symlinks_and_list_is_sorted() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        std::fs::create_dir_all(root.join("sub/.cache")).unwrap();
        std::fs::create_dir(root.join(".hidden")).unwrap();
        for f in [
            "b.txt",
            "a.txt",
            "sub/c.txt",
            "sub/.cache/x",
            ".hidden/y",
            ".env",
        ] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        std::os::unix::fs::symlink(root.join("a.txt"), root.join("link.txt")).unwrap();
        std::os::unix::fs::symlink(root.join("sub"), root.join("subl")).unwrap();
        let mut walked = walk(root);
        walked.sort();
        assert_eq!(walked, ["a.txt", "b.txt", "sub/c.txt"]);
        assert!(walk(&root.join("missing")).is_empty());
    }

    #[test]
    fn in_a_git_work_tree_ignored_files_are_not_candidates() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        let init = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["init", "-q"])
            .status();
        if !init.is_ok_and(|s| s.success()) {
            eprintln!("SKIPPED: git is not available");
            return;
        }
        std::fs::write(root.join(".gitignore"), "build/\n*.log\n").unwrap();
        std::fs::create_dir(root.join("build")).unwrap();
        for f in ["main.rs", "debug.log", "build/out.o", "untracked.md"] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        assert_eq!(list_files(root), ["main.rs", "untracked.md"]);
    }

    #[test]
    fn files_dir_must_exist_and_be_a_directory() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("f"), "x").unwrap();
        assert!(matches!(
            canonical_dir(&t.path().join("missing")),
            Err(GreviError::Input(_))
        ));
        assert!(matches!(
            canonical_dir(&t.path().join("f")),
            Err(GreviError::Input(_))
        ));
        assert!(canonical_dir(t.path()).is_ok());
    }
}
