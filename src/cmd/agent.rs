use crate::cli::Shell;
use crate::cmd::Outcome;
use crate::config::{Backend, Config};
use crate::exit::{Exit, JevifyError};

const GUIDE: &str = include_str!("../../docs/ROBOT_MODE.md");

pub fn capabilities() -> Outcome {
    let exit_codes: Vec<_> = Exit::ALL
        .iter()
        .map(|(e, d)| serde_json::json!({ "code": e.code(), "name": e, "meaning": d }))
        .collect();
    let data = serde_json::json!({
        "name": "jevify",
        "version": env!("CARGO_PKG_VERSION"),
        "summary": "Answers questions about text that already exists (your input, the installed tools, man pages, folders) by meaning. It selects and never generates. Backend-specific decision scores are not evidence of calibration for every task. Model: Jev through TypeSafe with a key or classifier.dev without one.",
        "use_when": "a question is about meaning and grep or keywords cannot ask it, or the input is too long to read; skip it when a literal search answers the question or you already know the exact command",
        "global_flags": ["--json (alias --robot)", "--format human|json|jsonl|toon", "-t/--threshold <0..1>", "--model <id>", "--no-cache", "-v/--verbose"],
        "output": "human stdout is plain text made for pipes (pick prints the line, is prints nothing), so there is no automatic switch to JSON when piped: pass --json to get the envelope",
        "commands": [
            { "name": "pick", "usage": "<stdin> | jevify pick \"<intent>\" [-n N] [--index]  |  jevify pick --files <DIR> \"<intent>\" [-n N]", "stdin": true, "exit": [0, 3], "data": "matches[{line,text,p}], any, source", "when": "choose one item out of many by description (a branch, a commit, a file, a process, a history line); the description and the line need no word in common. --files chooses among the files under DIR by what they are about (path names first, then the beginning of at most 24 finalist files) and prints a path", "example": "git log --oneline | jevify pick --json \"the commit that renamed the project\"" },
            { "name": "why", "usage": "<cmd> 2>&1 | jevify why [-C N] [-n N]  |  jevify why [-C N] -- <cmd...>", "stdin": true, "exit": [0, 3], "data": "causes[{line,text,p,context[]}], any, considered, total, hint, child_exit", "when": "root-cause a build, test or CI log, above all a long one or one where grep for error|fail found the symptom and not the reason", "example": "gh run view --log-failed | jevify why --json", "note": "past 1,500 distinct lines it keeps the neighbourhoods of error-like lines within a 4,000-line budget: compare considered with total" },
            { "name": "route", "usage": "jevify route <intent...>", "stdin": false, "exit": [0, 3], "data": "tool, summary, fit, alternatives[]", "when": "find the installed tool for a task", "example": "jevify route 'keep my mac awake for an hour'", "note": "prints a tool; starts no command" },
            { "name": "filter", "usage": "<stdin> | jevify filter [-v] [-c] [--strict] [-0|--para] [--files] [--no-save] '<statement>'", "stdin": true, "when": "keep matching records", "example": "cargo test 2>&1 | jevify filter 'reports a failed assertion'", "note": "not implemented" },
            { "name": "is", "usage": "<stdin> | jevify is \"<condition>\" [--band 0.15]", "stdin": true, "exit": [0, 1, 3], "data": "p, verdict, truncated, reason (when oversized)", "when": "triage or gate on meaning; loop over bounded texts and read the exit codes", "example": "jevify is \"the customer is about to stop being a customer\" < ticket.txt; echo $?", "note": "oversized evidence abstains before API requests, with p null and a stderr warning" },
            { "name": "add", "usage": "jevify add [--dry-run|--yes] \"<topic>\"", "stdin": false, "exit": [0, 3, 6, 130], "data": "hunks[{file,header,p,staged}]", "when": "stage part of a working tree without a terminal: git add -p is interactive, add is not", "example": "jevify add --json --dry-run \"the token expiry fix\"", "note": "stages single hunks of tracked files; rejects oversized hunks or batches before requests or staging; index only, never commits; works from any subdirectory" },
            { "name": "sort", "usage": "jevify sort <dir> [--into <root>] [--apply | --undo <log>]", "stdin": false, "exit": [0, 3, 6], "data": "moves[{from,to,p}], skipped[{file,reason}], undo_log, applied", "when": "files whose names say nothing need a home among the folders that already exist; it reads an excerpt", "example": "jevify sort --json ~/Downloads", "note": "dry-run by default; atomic no-replace apply/undo; unique durable JSONL recovery journal; symlink entries skipped; same volume only; concurrent source replacement unsupported; failures identify recovery log and progress" },
            { "name": "capabilities", "usage": "jevify capabilities --json" },
            { "name": "robot-docs", "usage": "jevify robot-docs [guide|commands|exit-codes|examples|privacy]" },
            { "name": "health", "usage": "jevify health --json", "exit": [0, 4, 5] },
            { "name": "init", "usage": "eval \"$(jevify init zsh|bash)\"" }
        ],
        "common_exit": { "codes": [2, 4, 5, 6], "meaning": "any command: usage, API unavailable, auth, input" },
        "exit_codes": exit_codes,
        "env": [
            { "name": "TYPESAFE_API_KEY", "meaning": "API key (never printed); its presence selects the typesafe backend" },
            { "name": "TYPESAFE_API_KEY_FILE", "meaning": "path to a file holding the key (read only when a key is needed)" },
            { "name": "JEVIFY_BACKEND", "default": "typesafe with a key, classifier without one", "meaning": "typesafe|classifier: which API answers. Both run Jev; classifier.dev is free and needs no key" },
            { "name": "JEVIFY_BASE_URL", "default": "the active backend's own URL", "meaning": "HTTPS on port 443 at api.typesafe.ai for typesafe or classifier.dev for classifier; localhost/127.0.0.1 allow any scheme and port; no userinfo or redirects" },
            { "name": "JEVIFY_MODEL", "default": "jev-1.13.0", "meaning": "Default applies to TypeSafe model selection; jev-latest moves with each release. Explicit overrides are rejected on classifier.dev, which controls its model" },
            { "name": "JEVIFY_THRESHOLD", "default": 0.5 },
            { "name": "JEVIFY_CONCURRENCY", "default": 8 },
            { "name": "JEVIFY_CACHE_DIR", "default": "platform cache dir/jevify" },
            { "name": "JEVIFY_NO_CACHE", "meaning": "disable the answer cache (entries expire after 7 days anyway)" },
            { "name": "JEVIFY_PRICE_PER_MTOK", "default": 0.042 },
            { "name": "JEVIFY_INVENTORY_FILE", "meaning": "JSON array of {name, summary} replacing the PATH inventory (tests, evals)" },
            { "name": "JEVIFY_CNF", "meaning": "enable the command-not-found hook from `jevify init`" }
        ],
        "limits": { "choice_options": 255, "window": crate::tournament::WINDOW, "state_tokens": 32000, "request_tokens": 64000, "requests_per_minute": 1200, "tokens_per_second": 250000, "stdin_bytes": crate::input::MAX_BYTES, "pick_lines": crate::cmd::pick::MAX_LINES },
        "backends": [
            { "name": "typesafe", "key": "required", "model": "Jev", "window": Backend::Typesafe.window(), "choice_options": 255, "state_chars": "32k tokens", "requests_per_minute": 1200, "meta": "input_tokens is null unless every inference attempt reports usage; cost_usd estimates input-token cost at the configured price and is null when that basis is incomplete" },
            { "name": "classifier", "key": "none", "model": "service-controlled Jev; explicit model overrides unsupported", "decision_semantics": "two-label Choice substitutes for Noul; scores and thresholds are not assumed interchangeable with TypeSafe Noul", "window": Backend::Classifier.window(), "choice_options": crate::jev::classifier::MAX_LABELS, "state_chars": crate::jev::classifier::MAX_INPUT_CHARS, "questions_per_request": crate::jev::classifier::MAX_DIMENSIONS, "classifications_per_minute": 3000, "meta": "input_tokens is null when token usage is unavailable; cost_usd is 0 at the default zero service price, with an explicit telemetry.cost_estimate basis" }
        ],
        "envelope": { "fields": ["ok", "command", "version", "exit_code", "data", "meta{backend,model,elapsed_ms,requests,cache_hits,input_tokens,cost_usd,threshold,request_id,telemetry}", "error{kind,message,hint,example}"] },
        "telemetry": {
            "attempt_groups": ["inference_posts", "health_gets", "prewarm_gets", "semantic_calls"],
            "conservation": "attempted = succeeded + failed + cancelled + in_flight",
            "transport_success": "HTTP 200 with the full response body received; semantic parsing and validation are counted separately",
            "semantic_questions": "questions submitted to ask, including cache hits and locally rejected calls",
            "retries": "retry_sends counts sends after the initial attempt; retry_sleep_ms counts elapsed completed or interrupted retry waits",
            "usage": "input_tokens and output_tokens each expose reported_subtotal, reported_attempts, unknown_attempts, complete; reported_attempts + unknown_attempts = inference_posts.attempted; cache hits add no service usage",
            "logical_rounds": "null: client-level accounting cannot infer logical rounds",
            "cost_estimate": "configured input price and reported input subtotal; complete is false for unknown input usage unless the configured price is zero"
        },
        "phrasing": [
            "write what must be true of the text, literally: the statement is judged word for word (\"the customer is about to stop being a customer\" beats \"this customer is about to leave\", which also matches an employee who is leaving their company)",
            "describe the thing, not what you will do with it: \"the line with the failing assertion\", not \"what should I fix\"",
            "one question per call; for A or B, make two calls",
            "English works best; jevify does not count, do arithmetic, compare dates or judge quality"
        ],
        "workflows": [
            { "goal": "find the tool for a task", "command": "jevify run --json --dry-run \"<task>\"" },
            { "goal": "explain a failure", "command": "<cmd> 2>&1 | jevify why --json" },
            { "goal": "explain a failed CI run, however long the log", "command": "gh run view --log-failed | jevify why --json" },
            { "goal": "select an item", "command": "<list> | jevify pick --json \"<intent>\"" },
            { "goal": "branch in a script", "command": "jevify is \"<condition>\" < file; case $? in 0) ...;; 1) ...;; 3) ...;; esac" },
            { "goal": "triage many texts without reading them", "command": "for f in dir/*; do jevify is \"<statement>\" < \"$f\" >/dev/null 2>&1; echo \"$f $?\"; done   # 0 yes, 1 no, 3 unsure: read only those" },
            { "goal": "stage one topic out of a mixed working tree", "command": "jevify add --json --dry-run \"<topic>\"   # then --yes, when the user asked you to stage" }
        ],
        "safety": [
            "meta.requests counts attempted inference POSTs, including retries and failures, excluding prewarm and health GETs",
            "run complete=true requires exactly one argv token: true, false, pwd, or ls; all flags, operands, and other commands remain unvalidated proposals with complete=false and a blocked reason",
            "run resolves recipe executables through PATH, which must be trusted; a command name is not executable identity verification",
            "is abstains without an API request when input exceeds its evidence budget: exit 3, p null, verdict unsure, truncated true, and a reason; stderr warns that the whole input was not judged",
            "add rejects any hunk above 3000 characters or any complete batch exceeding the backend state budget before API requests or staging; it never classifies clipped evidence",
            "run executes only after TTY confirmation or --yes; in machine mode only with --exec --yes, and the child's stdout goes to stderr so stdout stays one envelope",
            "run never executes tools on its never-execute list (rm, dd, mkfs*, diskutil, shutdown, kill, sudo, wrappers such as sh/bash/env/xargs/find/timeout that would run another program, and interpreters such as python*/perl*/ruby*/node*/php*/lua* that take program text, ...): data.blocked names the reason and argv is only shown",
            "commands run via argv, never a shell; flags come from man pages, no binary is ever probed with --help",
            "obvious secrets are masked before text is sent (best effort)",
            "results are pointers into input, the machine, or man pages; nothing is generated"
        ]
    });
    Outcome {
        exit: Exit::Ok,
        human: format!("{}\n", serde_json::to_string_pretty(&data).unwrap()).into_bytes(),
        exec: None,
        data,
    }
}

pub fn robot_docs(topic: Option<&str>) -> Result<Outcome, JevifyError> {
    let caps = capabilities().data;
    let text = match topic.unwrap_or("guide") {
        "guide" => GUIDE.to_string(),
        "commands" => serde_json::to_string_pretty(&caps["commands"]).unwrap(),
        "exit-codes" => serde_json::to_string_pretty(&caps["exit_codes"]).unwrap(),
        "examples" => serde_json::to_string_pretty(&caps["workflows"]).unwrap(),
        "privacy" => include_str!("../../PRIVACY.md").to_string(),
        other => {
            return Err(JevifyError::Usage(format!(
                "unknown topic `{other}`; topics: guide, commands, exit-codes, examples, privacy"
            )));
        }
    };
    Ok(Outcome {
        exit: Exit::Ok,
        data: serde_json::json!({ "topic": topic.unwrap_or("guide"), "text": text }),
        human: format!("{text}\n").into_bytes(),
        exec: None,
    })
}

pub async fn health(ctx: &Config) -> Result<Outcome, JevifyError> {
    let base = crate::config::base_url(ctx.backend, Some(&ctx.base_url))?;
    // Both backends are probed the same way, at the cheapest endpoint each offers; only
    // TypeSafe needs a key, and only there can the answer be "the key is wrong".
    let (path, key) = match ctx.backend {
        Backend::Typesafe => ("/v1/models", Some(ctx.api_key()?)),
        Backend::Classifier => ("/v1/health", None),
    };
    let mut req = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| JevifyError::Unavailable(e.to_string()))?
        .get(format!("{base}{path}"))
        .timeout(std::time::Duration::from_secs(5));
    if let Some(k) = &key {
        req = req.bearer_auth(k);
    }
    let start = std::time::Instant::now();
    let mut attempt = ctx.stats.start(crate::jev::client::AttemptKind::Health);
    let r = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            attempt.finish(false);
            return Err(JevifyError::Unavailable(e.to_string()));
        }
    };
    let ms = start.elapsed().as_millis();
    let backend = ctx.backend.as_str();
    match r.status().as_u16() {
        200 => {
            let bytes = match r.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    attempt.finish(false);
                    return Err(JevifyError::Unavailable(e.to_string()));
                }
            };
            attempt.finish(true);
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
            let key_state = if key.is_some() {
                "present"
            } else {
                "not needed"
            };
            Ok(Outcome {
                exit: Exit::Ok,
                human: format!("ok: {backend} reachable in {ms} ms (key {key_state})\n")
                    .into_bytes(),
                exec: None,
                data: serde_json::json!({
                    "backend": backend,
                    "base_url": ctx.base_url,
                    "key": key_state,
                    "api": "reachable",
                    "latency_ms": ms,
                    "models": body["models"],
                }),
            })
        }
        401 | 403 => {
            attempt.finish(false);
            Err(JevifyError::BadKey(r.status().as_u16()))
        }
        s => {
            attempt.finish(false);
            Err(JevifyError::Unavailable(format!("HTTP {s}")))
        }
    }
}

pub fn init(shell: Shell) -> Outcome {
    if matches!(shell, Shell::Agents) {
        let capabilities = capabilities().data;
        let verbs = capabilities["commands"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|command| command["name"].as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let block = format!(
            "# jevify\nSelect existing items by meaning with: {verbs}.\nUse jevify capabilities --json as the source of truth for commands and flags.\n"
        );
        return Outcome {
            exit: Exit::Ok,
            data: serde_json::json!({ "script": block }),
            human: block.into_bytes(),
            exec: None,
        };
    }
    let body = match shell {
        Shell::Zsh => {
            r#"# jevify shell integration — add to ~/.zshrc: eval "$(jevify init zsh)"
alias ,='noglob jevify route'
# Opt-in: route unknown commands of 3+ words to jevify (export JEVIFY_CNF=1).
if [[ -n $JEVIFY_CNF ]] && ! (( $+functions[command_not_found_handler] )); then
  command_not_found_handler() {
    if (( $# >= 3 )); then jevify route "$*"; return $?; fi
    print -u2 "zsh: command not found: $1"; return 127
  }
fi
"#
        }
        Shell::Bash => {
            r#"# jevify shell integration — add to ~/.bashrc: eval "$(jevify init bash)"
alias ,='jevify route'
if [[ -n $JEVIFY_CNF ]] && ! declare -F command_not_found_handle >/dev/null; then
  command_not_found_handle() {
    if (( $# >= 3 )); then jevify route "$*"; return $?; fi
    echo "bash: $1: command not found" >&2; return 127
  }
fi
"#
        }
        Shell::Agents => unreachable!("handled above"),
    };
    Outcome {
        exit: Exit::Ok,
        data: serde_json::json!({ "script": body }),
        human: body.as_bytes().to_vec(),
        exec: None,
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
        // Both backends are documented, with the env var that picks one.
        let names: Vec<&str> = d["backends"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["typesafe", "classifier"]);
        assert!(
            d["env"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["name"] == "JEVIFY_BACKEND")
        );
        assert!(
            d["envelope"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f.as_str().unwrap().contains("meta{backend,"))
        );
        assert!(
            d["commands"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["name"].is_string() && c["usage"].is_string())
        );
        // An agent must learn from here when each verb is worth a call, with a command to copy.
        assert_eq!(
            d["commands"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            crate::VERBS
        );
        for verb in ["pick", "why", "route", "filter", "is", "add", "sort"] {
            let c = d["commands"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == verb)
                .unwrap();
            assert!(c["when"].as_str().is_some_and(|s| !s.is_empty()), "{c}");
            assert!(
                c["example"]
                    .as_str()
                    .is_some_and(|s| s.contains(&format!("jevify {verb}"))),
                "{c}"
            );
        }
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
            let script = String::from_utf8(init(shell).human).unwrap();
            assert!(script.contains("alias ,=") && script.contains("jevify route"));
            assert!(!script.contains("jevify run"));
        }
    }
    #[test]
    fn init_agents_names_every_verb_and_the_capability_contract() {
        let block = String::from_utf8(init(Shell::Agents).human).unwrap();
        for verb in crate::VERBS {
            assert!(block.contains(verb));
        }
        assert!(block.contains("jevify capabilities --json"));
        assert!(block.lines().count() <= 25);
    }
}
