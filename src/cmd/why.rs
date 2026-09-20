use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, JevifyError};
use crate::jev::client::Client;
use crate::tournament::{Prompts, rank};
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub const SMALL: usize = 1500;
pub const TAIL: usize = 1000;
pub const NEIGHBOURS: usize = 5;
pub const MAX_KEEP: usize = 4000;
/// A daemon left behind by the command (a build server, a file watcher) can keep the pipe open
/// after the command itself has exited; wait this long for it, then use what was captured.
const DAEMON_GRACE: Duration = Duration::from_secs(1);

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

/// Runs `cmd` via argv (never a shell) with stdout and stderr on one pipe, so the lines
/// interleave as they would on a terminal. Returns the lines and the child's exit code.
fn capture(cmd: &[String]) -> Result<(Vec<String>, Option<i32>), JevifyError> {
    use std::io::Read;
    use std::sync::{Arc, Mutex};
    let io = |e: std::io::Error| JevifyError::Input(e.to_string());
    let (mut reader, writer) = std::io::pipe().map_err(io)?;
    let err = writer.try_clone().map_err(io)?;
    // The temporary `Command` owns our two write ends and is dropped at the end of this
    // statement, so the child holds the only copies and EOF arrives when it exits.
    // The user's own `-- <cmd>` run via argv, never a shell: a false positive for the scanner.
    let mut child = std::process::Command::new(&cmd[0]) // ubs:ignore
        .args(&cmd[1..])
        .stdin(std::process::Stdio::null())
        .stdout(writer)
        .stderr(err)
        .spawn()
        .map_err(|e| JevifyError::Input(format!("failed to start {}: {e}", cmd[0])))?;
    // Reading happens on its own thread, so a child that has exited is not waited on forever
    // when a daemon it spawned still holds the write end (EOF would never come).
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buf);
    let reading = std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut b = sink.lock().unwrap();
                    b.extend_from_slice(&chunk[..n]);
                    if b.len() >= crate::input::MAX_BYTES {
                        break;
                    }
                }
            }
        }
    });
    let mut exited: Option<Instant> = None;
    let code = loop {
        if reading.is_finished() {
            if buf.lock().unwrap().len() >= crate::input::MAX_BYTES {
                // Nobody drains the pipe any more; a chatty child would block on write forever.
                let _ = child.kill();
            }
            // Already finished: joining cannot block. The daemon path below leaves the thread
            // detached on purpose, since a blocked `read` would never let a join return.
            let _ = reading.join();
            break child.wait().map_err(io)?.code();
        }
        if let Some(status) = child.try_wait().map_err(io)? {
            if exited.get_or_insert_with(Instant::now).elapsed() >= DAEMON_GRACE {
                eprintln!(
                    "jevify why: {} exited but a process it left behind still holds its output open; using what was captured",
                    cmd[0]
                );
                break status.code();
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let bytes = std::mem::take(&mut *buf.lock().unwrap());
    Ok((
        crate::input::split_lines(&String::from_utf8_lossy(&bytes)),
        code,
    ))
}

pub async fn run(
    ctx: &Config,
    context: usize,
    top: usize,
    cmd: &[String],
) -> Result<Outcome, JevifyError> {
    if top == 0 {
        return Err(JevifyError::Usage("-n must be at least 1".into()));
    }
    let client = Client::new(ctx)?;
    let (lines, child_exit) = if cmd.is_empty() {
        (crate::input::read_stdin_async().await?, None)
    } else {
        let cmd = cmd.to_vec();
        tokio::task::spawn_blocking(move || capture(&cmd))
            .await
            .map_err(|e| JevifyError::Input(e.to_string()))??
    };
    if lines.iter().all(|l| l.trim().is_empty()) {
        return Err(JevifyError::EmptyInput("the command printed nothing"));
    }
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
    let hint = (causes.is_empty() && no_signal && cmd.is_empty()).then(|| {
        "no error-like lines on stdin; most tools write errors to stderr: `cmd 2>&1 | jevify why` or `jevify why -- cmd`".to_string()
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
        data: serde_json::json!({ "causes": causes, "any": ranking.any, "considered": kept.len(), "total": lines.len(), "hint": hint, "child_exit": child_exit }),
        human,
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
    #[test]
    fn capture_interleaves_stdout_and_stderr_and_keeps_the_exit_code() {
        let cmd: Vec<String> = ["sh", "-c", "echo out; echo err >&2; exit 3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let t0 = Instant::now();
        let (lines, code) = capture(&cmd).unwrap();
        // Under the grace period: this return must come from EOF, not from the daemon fallback.
        assert!(t0.elapsed() < DAEMON_GRACE, "EOF not seen");
        assert_eq!(lines, ["out", "err"]);
        assert_eq!(code, Some(3));
    }
    // A background process that inherits the pipe must not make `why -- cmd` wait for it.
    #[test]
    fn capture_returns_once_the_child_exits_even_if_a_daemon_keeps_the_pipe_open() {
        let cmd: Vec<String> = ["sh", "-c", "sleep 10 & echo out; exit 2"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let t0 = Instant::now();
        let (lines, code) = capture(&cmd).unwrap();
        assert!(
            t0.elapsed() < Duration::from_secs(8),
            "waited for the daemon"
        );
        assert_eq!(lines, ["out"]);
        assert_eq!(code, Some(2));
    }
}
