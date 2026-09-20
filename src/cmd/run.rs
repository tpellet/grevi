use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, GreviError};
use crate::inventory::{self, Tool};
use crate::jev::client::Client;
use crate::jev::{Question, Questions};
use crate::manpage;
use crate::tournament::{Prompts, shortlist};

/// Never executed by grevi, whatever the confidence or the flags: shown as a proposal instead.
/// Matched by tool name; `NEVER_EXEC_PREFIX` covers families such as `mkfs.ext4` and the
/// interpreters (`python3.12`, `php8.2`, `lua5.4`), which Linux distributions ship versioned.
const NEVER_EXEC: &[&str] = &[
    "rm",
    "rmdir",
    "dd",
    "fdisk",
    "diskutil",
    "shred",
    "srm",
    "wipefs",
    "sudo",
    "su",
    "doas",
    "kill",
    "killall",
    "pkill",
    "reboot",
    "halt",
    "shutdown",
    "poweroff",
    "init",
    "telinit",
    "launchctl",
    "systemctl",
    // Wrappers run another program named in their arguments (`bash -c "rm …"`, `xargs rm`,
    // `find -exec`); by name like the rest, or they would bypass the list.
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "fish",
    "env",
    "xargs",
    "nohup",
    "nice",
    "timeout",
    "time",
    "exec",
    "eval",
    "command",
    "find",
    "watch",
    "parallel",
    "osascript",
];
// Script interpreters take program text as a flag value (`python3 -c`, `perl -e`, `node -e`);
// prefixes, because Linux ships them versioned (`perl5.36`, `ruby3.1`, `nodejs`, `php8.2`,
// `lua5.4`) and exact names would bypass the list there. False positives (`phpunit`, `luacheck`)
// fail safe: shown, never run.
const NEVER_EXEC_PREFIX: &[&str] = &[
    "mkfs", "newfs", "python", "perl", "ruby", "node", "php", "lua",
];

pub struct RunFlags {
    pub yes: bool,
    pub exec: bool,
    pub dry_run: bool,
    pub no_args: bool,
    pub machine: bool,
}

pub struct Route {
    pub tool: Option<Tool>,
    pub fit: f64,
    pub alternatives: Vec<(String, f64)>,
}

fn load_tools(cache_dir: Option<std::path::PathBuf>) -> Result<Vec<Tool>, GreviError> {
    if let Ok(p) = std::env::var("GREVI_INVENTORY_FILE") {
        let b = std::fs::read(&p)
            .map_err(|e| GreviError::Input(format!("GREVI_INVENTORY_FILE: {e}")))?;
        return serde_json::from_slice(&b)
            .map_err(|e| GreviError::Input(format!("GREVI_INVENTORY_FILE: {e}")));
    }
    inventory::load(cache_dir.as_deref())
}

pub async fn route(
    client: &Client,
    ctx: &Config,
    request: &str,
    tools: &[Tool],
) -> Result<Route, GreviError> {
    let items: Vec<String> = tools
        .iter()
        .map(|t| format!("{}: {}", t.name, t.summary))
        .collect();
    let prompts = Prompts {
        choose: "Which command in `items` is the right tool to accomplish `request`? Choose NONE if no listed command does it.".into(),
        none: "none of the listed commands does what the request asks".into(),
        any: "Is there a command in `items` whose purpose is to accomplish `request`?".into(),
    };
    // Round 1: windows only. The absolute fit Nouls below are round 2, so no Choice finals round.
    let finalists: Vec<usize> = shortlist(client, request, &items, &prompts, 3)
        .await?
        .iter()
        .take(12)
        .map(|c| c.index)
        .collect();
    if finalists.is_empty() {
        return Ok(Route {
            tool: None,
            fit: 0.0,
            alternatives: vec![],
        });
    }
    // Round 2: absolute fit per finalist, with a richer man-page excerpt.
    // `man` costs ~90 ms per page; render the finalists' pages in parallel, not in series.
    let described: Vec<String> = std::thread::scope(|s| {
        let handles: Vec<_> = finalists
            .iter()
            .map(|&i| {
                let t = &tools[i];
                s.spawn(move || match manpage::description(&t.name, 500) {
                    Some(d) => format!("{}: {}. {}", t.name, t.summary, d),
                    None => format!("{}: {}", t.name, t.summary),
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_default())
            .collect()
    });
    let state = serde_json::json!({ "request": request, "commands": described });
    let mut qs = Questions::new();
    for (k, _) in finalists.iter().enumerate() {
        qs.insert(
            format!("fit{k:02}"),
            Question::noul_with(
                // What the command is, never what running it would do: a counterfactual Noul
                // sits at 0.33–0.59 whatever the input (measured on jev-1.13).
                format!("Is the command described in `commands[{k}]` a correct, direct way to accomplish `request`?"),
                "this command is a correct, direct way to accomplish the request",
                "this command does something else, or is only tangentially related",
            ),
        );
    }
    let r = client.ask(&state, &qs).await?;
    // A missing answer is a protocol error, not "low confidence".
    let mut fits: Vec<(usize, f64)> = Vec::with_capacity(finalists.len());
    for (k, &i) in finalists.iter().enumerate() {
        fits.push((i, r.noul(&format!("fit{k:02}"))?));
    }
    fits.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (best, fit) = fits[0];
    let alternatives = fits
        .iter()
        .skip(1)
        .take(4)
        .map(|(i, p)| (tools[*i].name.clone(), *p))
        .collect();
    let tool = (fit >= ctx.threshold).then(|| tools[best].clone());
    Ok(Route {
        tool,
        fit,
        alternatives,
    })
}

pub async fn run(ctx: &Config, intent: &str, flags: RunFlags) -> Result<Outcome, GreviError> {
    let client = Client::new(ctx)?;
    // Open the connection now; the inventory read below is the local work it overlaps with
    // (measured in benchmarks/README.md; the other verbs have no such work and do not prewarm).
    client.prewarm();
    let cache_dir = ctx.cache_dir.clone();
    let tools = tokio::task::spawn_blocking(move || load_tools(cache_dir))
        .await
        .map_err(|e| GreviError::Input(e.to_string()))??;
    let r = route(&client, ctx, intent, &tools).await?;
    let alts: Vec<_> = r
        .alternatives
        .iter()
        .map(|(n, p)| serde_json::json!({ "tool": n, "fit": p }))
        .collect();
    let Some(tool) = r.tool else {
        if !flags.machine {
            eprintln!("grevi: nothing installed does this (best fit {:.2})", r.fit);
            for (n, p) in r.alternatives.iter().take(3) {
                eprintln!("  closest: {n} ({p:.2})");
            }
        }
        return Ok(Outcome {
            exit: Exit::Abstain,
            data: serde_json::json!({ "tool": null, "fit": r.fit, "alternatives": alts }),
            human: String::new(),
        });
    };
    let mut argv = vec![tool.name.clone()];
    let mut placeholders = 0;
    let mut chosen_flags = serde_json::Value::Array(vec![]);
    let mut parsed: Vec<manpage::Flag> = Vec::new();
    let mut chosen: Vec<String> = Vec::new();
    if !flags.no_args {
        // Flags come from the man page only; grevi never runs a binary with --help to learn them.
        let name = tool.name.clone();
        parsed = tokio::task::spawn_blocking(move || {
            manpage::parse_flags(&manpage::options_text(&name).unwrap_or_default())
        })
        .await
        .map_err(|e| GreviError::Input(e.to_string()))?;
        let cwd: Vec<String> = std::fs::read_dir(".")
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| !n.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default();
        let files = relevant_files(intent, &cwd);
        if !parsed.is_empty() || !files.is_empty() {
            let p = crate::args::propose(&client, intent, &tool, &parsed, &files, ctx.threshold)
                .await?;
            argv = p.argv;
            placeholders = p.placeholders;
            chosen = p.flags.iter().map(|(f, _)| f.clone()).collect();
            chosen_flags = serde_json::json!(
                p.flags
                    .iter()
                    .map(|(f, p)| serde_json::json!({ "flag": f, "p": p }))
                    .collect::<Vec<_>>()
            );
        }
    }
    let shown = argv.join(" ");
    let complete = placeholders == 0;
    let blocked = blocked_reason(&tool.name);
    if !flags.machine {
        if flags.dry_run {
            eprintln!("grevi: {} ({:.2}) — {}", tool.name, r.fit, tool.summary);
        }
        // Each chosen flag with its man-page line, so the user can check the proposal.
        for f in parsed.iter().filter(|f| chosen.contains(&f.flag)) {
            eprintln!("  {}  {}", f.flag, f.desc);
        }
        if !complete {
            eprintln!("grevi: fill the <VALUE> placeholders and run it yourself:");
        } else if let Some(why) = &blocked {
            eprintln!("grevi: not offering to run this ({why}); check it and run it yourself:");
        }
    }
    let may_execute = complete
        && blocked.is_none()
        && !flags.dry_run
        && if flags.machine {
            flags.exec && flags.yes
        } else if flags.yes {
            true
        } else {
            let prompt = format!(
                "grevi: {} ({:.2})\n  {shown}\nRun it? [y/N] ",
                tool.name, r.fit
            );
            match crate::cmd::confirm_tty(&prompt)? {
                Some(true) => true,
                Some(false) => return Err(GreviError::Declined),
                // No TTY: print the proposal, never run it.
                None => false,
            }
        };
    let mut executed = false;
    let mut child_code: Option<i32> = None;
    if may_execute {
        let mut child = std::process::Command::new(&argv[0]);
        child.args(&argv[1..]);
        if flags.machine {
            // stdout carries exactly one envelope; the child's stdout goes to stderr.
            child.stdout(std::io::stderr());
        }
        let status = child
            .status()
            .map_err(|e| GreviError::Input(format!("failed to start {}: {e}", argv[0])))?;
        executed = true;
        child_code = status.code();
    }
    let exit = match (executed, child_code) {
        (true, Some(0)) | (false, _) => Exit::Ok,
        _ => Exit::ChildFailed,
    };
    Ok(Outcome {
        exit,
        data: serde_json::json!({ "tool": tool.name, "summary": tool.summary, "fit": r.fit, "argv": argv, "flags": chosen_flags,
                                  "complete": complete, "blocked": blocked, "executed": executed, "child_exit": child_code, "alternatives": alts }),
        human: if executed {
            String::new()
        } else {
            format!("{shown}\n")
        },
    })
}

/// Why a proposal is only shown, never offered for execution (None = it may be offered).
pub fn blocked_reason(tool: &str) -> Option<String> {
    let base = tool.rsplit('/').next().unwrap_or(tool);
    (NEVER_EXEC.contains(&base) || NEVER_EXEC_PREFIX.iter().any(|p| base.starts_with(p)))
        .then(|| format!("{base} is on grevi's never-execute list"))
}

/// Only cwd entries the request plausibly names leave the machine (and can be appended).
/// Sorted: a directory listing arrives in filesystem order, and option order moves an uncertain
/// probability (up to 0.23 measured), so the order must be canonical and part of the cache key.
pub fn relevant_files(intent: &str, cwd: &[String]) -> Vec<String> {
    let lower = intent.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .collect();
    let mut out: Vec<String> = cwd
        .iter()
        .filter(|n| {
            let n = n.to_lowercase();
            words.iter().any(|w| n.contains(w))
        })
        .cloned()
        .collect();
    out.sort();
    out.truncate(200);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn never_exec_list_blocks_by_name_and_prefix() {
        assert!(blocked_reason("rm").is_some());
        assert!(blocked_reason("/bin/rm").is_some());
        assert!(blocked_reason("mkfs.ext4").is_some());
        // Wrappers would otherwise run `rm` for the list: `bash -c`, `xargs`, `find -exec`;
        // interpreters carry program text in a flag value: `python3 -c`, `perl -e`; Linux ships
        // them versioned (`lua5.4`, `php8.2`, `ruby3.1`, `nodejs`), so they match by prefix.
        for wrapper in [
            "bash",
            "sh",
            "env",
            "xargs",
            "find",
            "timeout",
            "python",
            "python3",
            "python3.12",
            "perl",
            "perl5.36",
            "ruby3.1",
            "node",
            "nodejs",
            "php8.2",
            "lua5.4",
        ] {
            assert!(blocked_reason(wrapper).is_some(), "{wrapper}");
        }
        assert!(blocked_reason("tar").is_none());
    }
    #[test]
    fn relevant_files_keeps_only_names_the_request_mentions() {
        let cwd = ["ubuntu.iso".to_string(), "taxes-2025.pdf".to_string()];
        assert_eq!(
            relevant_files("burn a dvd from this iso", &cwd),
            ["ubuntu.iso"]
        );
        // Canonical order whatever order the directory listing came in.
        let unsorted = ["z.iso".to_string(), "a.iso".to_string()];
        assert_eq!(relevant_files("this iso", &unsorted), ["a.iso", "z.iso"]);
    }
}
