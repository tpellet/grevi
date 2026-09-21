#![deny(unsafe_code)]

pub mod args;
pub mod cli;
pub mod cmd;
pub mod config;
pub mod exit;
pub mod gitdiff;
pub mod input;
pub mod inventory;
pub mod jev;
pub mod manpage;
pub mod output;
pub mod records;
pub mod save;
pub mod tournament;

use clap::Parser;
use cli::{Cli, Cmd};
use exit::{Exit, JevifyError};
use output::{Envelope, ErrorBody, Format, Meta};
use std::ffi::OsString;
use std::io::Write;
use std::time::Instant;

const VERBS: [&str; 11] = [
    "pick",
    "why",
    "route",
    "filter",
    "is",
    "add",
    "sort",
    "capabilities",
    "robot-docs",
    "health",
    "init",
];

/// What bare `jevify` prints: enough to make a first call, in about 130 tokens. `--help` has the rest.
pub const QUICK_START: &str = concat!(
    "jevify ",
    env!("CARGO_PKG_VERSION"),
    r#": answer questions about text you already have. Selects, never generates.
  <list> | jevify pick "<description>"    find one line by meaning
  <cmd> 2>&1 | jevify why                 find the line that caused a failure
  jevify is "<statement>" < file          yes / no / unsure as exit code 0 / 1 / 3
  jevify route "<task>"                  find the installed command for a task
  <list> | jevify filter "<statement>"    keep matching records
  jevify add --dry-run "<topic>"          stage only the git changes about a topic
  jevify sort <dir>                       propose a folder for each file (dry run)
Add --json for one JSON object on stdout. No key needed.
Exit: 0 ok, 1 no, 2 usage, 3 nothing fits or unsure, 4 API unavailable, 5 auth, 6 input.
More: jevify <verb> --help | jevify --help | agents: jevify capabilities --json, jevify robot-docs
"#
);

pub fn main_exit() -> i32 {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    // Bare `jevify` stays a usage error (exit 2, stderr), as it was with clap's full help.
    if std::env::args_os().len() == 1 {
        eprint!("{QUICK_START}");
        return Exit::Usage.code();
    }
    let name = raw_command(&args);
    let removed = if name == "run" {
        Some(("jevify", "use jevify route 'x'"))
    } else if name == "why" && args.iter().any(|a| a == "--") {
        Some(("why", "use CMD 2>&1 | jevify why"))
    } else {
        None
    };
    if let Some((command, message)) = removed {
        if let Some(format) = machine_format(&args) {
            return report_error(
                format,
                command,
                &JevifyError::Usage(message.into()),
                Meta::default(),
            );
        }
        eprintln!("jevify: {message}");
        return Exit::Usage.code();
    }
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            // A usage error under --json must still be exactly one envelope, not clap's text.
            if e.use_stderr() {
                if let Some(format) = machine_format(&args) {
                    let name = if VERBS.contains(&name) {
                        name
                    } else {
                        "jevify"
                    };
                    let message = e
                        .to_string()
                        .lines()
                        .next()
                        .unwrap_or("usage error")
                        .trim_start_matches("error: ")
                        .to_string();
                    return report_error(
                        format,
                        name,
                        &JevifyError::Usage(message),
                        Meta::default(),
                    );
                }
            }
            if let Err(error) = e.print() {
                return if e.use_stderr() {
                    Exit::Usage.code()
                } else {
                    stdout_error(error)
                };
            }
            return if e.use_stderr() {
                Exit::Usage.code()
            } else {
                Exit::Ok.code()
            };
        }
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(run_cli(cli))
}

/// Clap failed before a `Cli` existed, so the requested machine format is read from the raw args.
fn machine_format(args: &[OsString]) -> Option<Format> {
    use clap::ValueEnum;
    let mut json = false;
    let mut format = None;
    let mut it = args.iter().take_while(|arg| *arg != "--");
    while let Some(a) = it.next() {
        let Some(a) = a.to_str() else { continue };
        match a {
            "--json" | "--robot" => json = true,
            "--format" => {
                format = it
                    .next()
                    .and_then(|v| v.to_str())
                    .and_then(|v| Format::from_str(v, true).ok())
            }
            other => {
                if let Some(v) = other.strip_prefix("--format=") {
                    format = Format::from_str(v, true).ok();
                }
            }
        }
    }
    format
        .or(json.then_some(Format::Json))
        .filter(|f| *f != Format::Human)
}

fn raw_command(args: &[OsString]) -> &str {
    let mut args = args.iter().take_while(|arg| *arg != "--");
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--format" | "--threshold" | "-t" | "--model") => {
                args.next();
            }
            Some(value) if value.starts_with('-') => {}
            Some(value) => return value,
            None => return "jevify",
        }
    }
    "jevify"
}

fn command_name(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::Pick { .. } => "pick",
        Cmd::Why { .. } => "why",
        Cmd::Route { .. } => "route",
        Cmd::Filter { .. } => "filter",
        Cmd::Is { .. } => "is",
        Cmd::Add { .. } => "add",
        Cmd::Sort { .. } => "sort",
        Cmd::Capabilities => "capabilities",
        Cmd::RobotDocs { .. } => "robot-docs",
        Cmd::Health => "health",
        Cmd::Init { .. } => "init",
    }
}

async fn run_cli(cli: Cli) -> i32 {
    let start = Instant::now();
    let format = cli.g.format();
    let name = command_name(&cli.cmd);
    let ctx = match config::Config::load(&cli.g) {
        Ok(c) => c,
        Err(e) => return report_error(format, name, &e, Meta::default()),
    };
    let result = dispatch(&cli, &ctx).await;
    let mut meta = ctx.meta();
    meta.elapsed_ms = start.elapsed().as_millis();
    match result {
        Ok(out) => {
            debug_assert!(out.exec.is_none());
            if format == Format::Human {
                if let Err(e) = std::io::stdout().lock().write_all(&out.human) {
                    return stdout_error(e);
                }
                if cli.g.verbose {
                    eprintln!("jevify: {}", out.data);
                    eprintln!(
                        "jevify: {} ms, {} requests, {} cached, {}",
                        meta.elapsed_ms,
                        meta.requests,
                        meta.cache_hits,
                        meta.cost_usd
                            .map(|cost| format!("${cost:.5}"))
                            .unwrap_or_else(|| "cost unknown".into())
                    );
                }
            } else {
                let env = Envelope {
                    ok: true,
                    command: name,
                    version: env!("CARGO_PKG_VERSION"),
                    exit_code: out.exit.code(),
                    data: out.data,
                    meta,
                    error: None,
                };
                if let Err(e) = writeln!(
                    std::io::stdout().lock(),
                    "{}",
                    output::render(format, &env).expect("render envelope")
                ) {
                    return stdout_error(e);
                }
            }
            out.exit.code()
        }
        Err(e) => report_error(format, name, &e, meta),
    }
}

fn report_error(format: Format, name: &str, e: &JevifyError, meta: Meta) -> i32 {
    if format == Format::Human {
        eprintln!(
            "jevify {name}: error: {e}\n  hint: {}\n  try:  {}",
            e.hint(),
            e.example()
        );
        if let Some(id) = &meta.request_id {
            eprintln!("  request: {id}");
        }
    } else {
        let env = Envelope {
            ok: false,
            command: name,
            version: env!("CARGO_PKG_VERSION"),
            exit_code: e.exit().code(),
            data: serde_json::Value::Null,
            meta,
            error: Some(ErrorBody {
                kind: e.kind(),
                message: e.to_string(),
                hint: e.hint(),
                example: e.example(),
            }),
        };
        if let Err(e) = writeln!(
            std::io::stdout().lock(),
            "{}",
            output::render(format, &env).expect("render envelope")
        ) {
            return stdout_error(e);
        }
    }
    e.exit().code()
}

fn stdout_error(error: std::io::Error) -> i32 {
    if error.kind() == std::io::ErrorKind::BrokenPipe {
        Exit::Ok.code()
    } else {
        let _ = writeln!(std::io::stderr().lock(), "jevify: output error: {error}");
        Exit::Input.code()
    }
}

/// Every verb is wired here once (Task 1). Later tasks replace stub bodies in `cmd/*.rs`
/// and never edit this function.
async fn dispatch(cli: &Cli, ctx: &config::Config) -> Result<cmd::Outcome, JevifyError> {
    let machine = cli.g.format() != Format::Human;
    match &cli.cmd {
        Cmd::Pick {
            intent,
            top,
            index,
            files,
            nul,
            para,
        } => cmd::pick::run(ctx, intent, *top, *index, split(*nul, *para), *files).await,
        Cmd::Why {
            context,
            top,
            no_save,
        } => cmd::why::run(ctx, *context, *top, *no_save).await,
        Cmd::Route { intent } => cmd::run::run(ctx, &intent.join(" "), machine).await,
        Cmd::Filter {
            statement,
            invert,
            count,
            strict,
            nul,
            para,
            files,
            no_save,
        } => {
            cmd::filter::run(
                ctx,
                statement,
                cmd::filter::FilterFlags {
                    invert: *invert,
                    count: *count,
                    strict: *strict,
                    split: split(*nul, *para),
                    files: *files,
                    no_save: *no_save,
                },
                machine,
            )
            .await
        }
        Cmd::Is {
            statements,
            context,
            band,
        } => cmd::is::run(ctx, statements, context.as_deref(), *band).await,
        Cmd::Add {
            topic,
            yes,
            dry_run,
        } => cmd::add::run(ctx, topic, *yes, *dry_run, machine).await,
        Cmd::Sort {
            dir,
            into,
            apply,
            undo,
        } => cmd::sort::run(ctx, dir, into.as_deref(), *apply, undo.as_deref()).await,
        Cmd::Capabilities => Ok(cmd::agent::capabilities()),
        Cmd::RobotDocs { topic } => cmd::agent::robot_docs(topic.as_deref()),
        Cmd::Health => cmd::agent::health(ctx).await,
        Cmd::Init { shell } => Ok(cmd::agent::init(*shell)),
    }
}

fn split(nul: bool, para: bool) -> records::Split {
    if nul {
        records::Split::Nul
    } else if para {
        records::Split::Para
    } else {
        records::Split::Lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_errors_only_succeed_for_broken_pipe() {
        assert_eq!(stdout_error(std::io::ErrorKind::BrokenPipe.into()), 0);
        assert_eq!(stdout_error(std::io::ErrorKind::PermissionDenied.into()), 6);
    }
    fn args(a: &[&str]) -> Vec<OsString> {
        a.iter().map(OsString::from).collect()
    }
    #[test]
    fn machine_format_is_read_from_raw_args_when_clap_fails() {
        assert_eq!(machine_format(&args(&["--format", "--", "--json"])), None);
        assert_eq!(raw_command(&args(&["--format", "--", "run"])), "jevify");
        assert_eq!(machine_format(&args(&["pick", "--nope"])), None);
        assert_eq!(
            machine_format(&args(&["--json", "pick"])),
            Some(Format::Json)
        );
        assert_eq!(
            machine_format(&args(&["pick", "--robot"])),
            Some(Format::Json)
        );
        assert_eq!(
            machine_format(&args(&["--format", "toon", "pick"])),
            Some(Format::Toon)
        );
        assert_eq!(
            machine_format(&args(&["--json", "--format=jsonl"])),
            Some(Format::Jsonl)
        );
        // `--format human` is not machine output, even next to --json.
        assert_eq!(
            machine_format(&args(&["--json", "--format", "human"])),
            None
        );
    }
}
