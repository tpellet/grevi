use crate::cli::Shell;
use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, HunchError};

const GUIDE: &str = include_str!("../../docs/ROBOT_MODE.md");

pub fn capabilities() -> Outcome {
    let exit_codes: Vec<_> = Exit::ALL
        .iter()
        .map(|(e, d)| serde_json::json!({ "code": e.code(), "name": e, "meaning": d }))
        .collect();
    let data = serde_json::json!({
        "name": "hunch",
        "version": env!("CARGO_PKG_VERSION"),
        "summary": "Point at the right thing among real things, by meaning, with calibrated confidence (TypeSafe Jev).",
        "global_flags": ["--json (alias --robot)", "--format human|json|jsonl|toon", "-t/--threshold <0..1>", "--model <id>", "--no-cache", "-v/--verbose"],
        "commands": [
            { "name": "pick", "usage": "<stdin> | hunch pick \"<intent>\" [-n N] [--index]", "stdin": true, "exit": [0, 3], "data": "matches[{line,text,p}], any" },
            { "name": "why", "usage": "<cmd> 2>&1 | hunch why [-C N] [-n N]  |  hunch why [-C N] -- <cmd...>", "stdin": true, "exit": [0, 3], "data": "causes[{line,text,p,context[]}], any, considered, total, hint, child_exit" },
            { "name": "run", "usage": "hunch run [--dry-run|--yes|--exec --yes] [--no-args] <intent...>", "stdin": false, "exit": [0, 3, 7, 130], "data": "tool, fit, argv[], flags[], complete, blocked, executed, child_exit, alternatives[]" },
            { "name": "is", "usage": "<stdin> | hunch is \"<condition>\" [--band 0.15]", "stdin": true, "exit": [0, 1, 3], "data": "p, verdict, truncated" },
            { "name": "capabilities", "usage": "hunch capabilities --json" },
            { "name": "robot-docs", "usage": "hunch robot-docs [guide|commands|exit-codes|examples|privacy]" },
            { "name": "health", "usage": "hunch health --json", "exit": [0, 4, 5] },
            { "name": "init", "usage": "eval \"$(hunch init zsh|bash)\"" }
        ],
        "common_exit": { "codes": [2, 4, 5, 6], "meaning": "any command: usage, API unavailable, auth, input" },
        "exit_codes": exit_codes,
        "env": [
            { "name": "TYPESAFE_API_KEY", "meaning": "API key (never printed)" },
            { "name": "TYPESAFE_API_KEY_FILE", "meaning": "path to a file holding the key (read only when a key is needed)" },
            { "name": "HUNCH_BASE_URL", "default": "https://api.typesafe.ai" },
            { "name": "HUNCH_MODEL", "default": "jev-1.13.0", "meaning": "pinned; `jev-latest` is an alias that moves with each release" },
            { "name": "HUNCH_THRESHOLD", "default": 0.5 },
            { "name": "HUNCH_CONCURRENCY", "default": 8 },
            { "name": "HUNCH_CACHE_DIR", "default": "platform cache dir/hunch" },
            { "name": "HUNCH_NO_CACHE", "meaning": "disable the answer cache (entries expire after 7 days anyway)" },
            { "name": "HUNCH_PRICE_PER_MTOK", "default": 0.042 },
            { "name": "HUNCH_NO_PREWARM", "meaning": "do not open the connection early" },
            { "name": "HUNCH_INVENTORY_FILE", "meaning": "JSON array of {name, summary} replacing the PATH inventory (tests, evals)" },
            { "name": "HUNCH_CNF", "meaning": "enable the command-not-found hook from `hunch init`" }
        ],
        "limits": { "choice_options": 255, "window": crate::tournament::WINDOW, "state_tokens": 32000, "request_tokens": 64000, "requests_per_minute": 1200, "tokens_per_second": 250000, "stdin_bytes": crate::input::MAX_BYTES, "pick_lines": crate::cmd::pick::MAX_LINES },
        "envelope": { "fields": ["ok", "command", "version", "exit_code", "data", "meta{model,elapsed_ms,requests,cache_hits,input_tokens,cost_usd,threshold,request_id}", "error{kind,message,hint,example}"] },
        "workflows": [
            { "goal": "find the tool for a task", "command": "hunch run --json --dry-run \"<task>\"" },
            { "goal": "explain a failure", "command": "<cmd> 2>&1 | hunch why --json" },
            { "goal": "select an item", "command": "<list> | hunch pick --json \"<intent>\"" },
            { "goal": "branch in a script", "command": "hunch is \"<condition>\" < file; case $? in 0) ...;; 1) ...;; 3) ...;; esac" }
        ],
        "safety": [
            "run executes only after TTY confirmation or --yes; in machine mode only with --exec --yes, and the child's stdout goes to stderr so stdout stays one envelope",
            "run never executes tools on its never-execute list (rm, dd, mkfs*, diskutil, shutdown, kill, sudo, wrappers such as sh/bash/env/xargs/find/timeout that would run another program, and interpreters such as python*/perl*/ruby*/node*/php*/lua* that take program text, ...): data.blocked names the reason and argv is only shown",
            "commands run via argv, never a shell; flags come from man pages, no binary is ever probed with --help",
            "obvious secrets are masked before text is sent (best effort)",
            "results are pointers into input, the machine, or man pages; nothing is generated"
        ]
    });
    Outcome {
        exit: Exit::Ok,
        human: format!("{}\n", serde_json::to_string_pretty(&data).unwrap()),
        data,
    }
}

pub fn robot_docs(topic: Option<&str>) -> Result<Outcome, HunchError> {
    let caps = capabilities().data;
    let text = match topic.unwrap_or("guide") {
        "guide" => GUIDE.to_string(),
        "commands" => serde_json::to_string_pretty(&caps["commands"]).unwrap(),
        "exit-codes" => serde_json::to_string_pretty(&caps["exit_codes"]).unwrap(),
        "examples" => serde_json::to_string_pretty(&caps["workflows"]).unwrap(),
        "privacy" => include_str!("../../PRIVACY.md").to_string(),
        other => {
            return Err(HunchError::Usage(format!(
                "unknown topic `{other}`; topics: guide, commands, exit-codes, examples, privacy"
            )));
        }
    };
    Ok(Outcome {
        exit: Exit::Ok,
        data: serde_json::json!({ "topic": topic.unwrap_or("guide"), "text": text }),
        human: format!("{text}\n"),
    })
}

pub async fn health(ctx: &Config) -> Result<Outcome, HunchError> {
    let key = ctx.api_key()?;
    let start = std::time::Instant::now();
    let r = reqwest::Client::new()
        .get(format!("{}/v1/models", ctx.base_url))
        .bearer_auth(&key)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| HunchError::Unavailable(e.to_string()))?;
    let ms = start.elapsed().as_millis();
    match r.status().as_u16() {
        200 => {
            let models: serde_json::Value = r.json().await.unwrap_or_default();
            Ok(Outcome {
                exit: Exit::Ok,
                human: format!("ok: key accepted, API reachable in {ms} ms\n"),
                data: serde_json::json!({ "key": "present", "api": "reachable", "latency_ms": ms, "models": models["models"] }),
            })
        }
        401 | 403 => Err(HunchError::BadKey(r.status().as_u16())),
        s => Err(HunchError::Unavailable(format!("HTTP {s}"))),
    }
}

pub fn init(shell: Shell) -> Outcome {
    let body = match shell {
        Shell::Zsh => {
            r#"# hunch shell integration — add to ~/.zshrc: eval "$(hunch init zsh)"
alias ,='noglob hunch run'
# Opt-in: route unknown commands of 3+ words to hunch (export HUNCH_CNF=1).
if [[ -n $HUNCH_CNF ]] && ! (( $+functions[command_not_found_handler] )); then
  command_not_found_handler() {
    if (( $# >= 3 )); then hunch run "$*"; return $?; fi
    print -u2 "zsh: command not found: $1"; return 127
  }
fi
"#
        }
        Shell::Bash => {
            r#"# hunch shell integration — add to ~/.bashrc: eval "$(hunch init bash)"
alias ,='hunch run'
if [[ -n $HUNCH_CNF ]] && ! declare -F command_not_found_handle >/dev/null; then
  command_not_found_handle() {
    if (( $# >= 3 )); then hunch run "$*"; return $?; fi
    echo "bash: $1: command not found" >&2; return 127
  }
fi
"#
        }
    };
    Outcome {
        exit: Exit::Ok,
        data: serde_json::json!({ "script": body }),
        human: body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capabilities_match_the_exit_contract_and_name_every_command() {
        let d = capabilities().data;
        // An observable value, not the list's own length echoed back: 130 is what an agent sees
        // when the user declines at the prompt.
        assert!(
            d["exit_codes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["code"] == 130),
            "{}",
            d["exit_codes"]
        );
        assert_eq!(d["limits"]["window"], crate::tournament::WINDOW);
        assert!(
            d["commands"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["name"].is_string() && c["usage"].is_string())
        );
    }
    #[test]
    fn robot_docs_default_to_the_guide_and_reject_unknown_topics() {
        assert_eq!(robot_docs(None).unwrap().data["topic"], "guide");
        // `Outcome` has no Debug, so `unwrap_err` is unavailable; `err()` needs only the error's.
        assert_eq!(robot_docs(Some("nope")).err().unwrap().exit(), Exit::Usage);
    }
    #[test]
    fn init_prints_the_comma_alias_for_both_shells() {
        for shell in [Shell::Zsh, Shell::Bash] {
            assert!(init(shell).human.contains("alias ,="));
        }
    }
}
