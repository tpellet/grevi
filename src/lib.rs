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
pub mod tournament;

use clap::Parser;
use cli::{Cli, Cmd};
use exit::{Exit, GreviError};
use output::{Envelope, ErrorBody, Format, Meta};
use std::time::Instant;

const VERBS: [&str; 10] = [
    "pick",
    "why",
    "run",
    "is",
    "add",
    "sort",
    "capabilities",
    "robot-docs",
    "health",
    "init",
];

pub fn main_exit() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            // A usage error under --json must still be exactly one envelope, not clap's text.
            let args: Vec<String> = std::env::args().skip(1).collect();
            if e.use_stderr() {
                if let Some(format) = machine_format(&args) {
                    let name = args
                        .iter()
                        .find(|a| VERBS.contains(&a.as_str()))
                        .map_or("grevi", String::as_str);
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
                        &GreviError::Usage(message),
                        Meta::default(),
                    );
                }
            }
            let _ = e.print();
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
fn machine_format(args: &[String]) -> Option<Format> {
    use clap::ValueEnum;
    let mut json = false;
    let mut format = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" | "--robot" => json = true,
            "--format" => format = it.next().and_then(|v| Format::from_str(v, true).ok()),
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

fn command_name(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::Pick { .. } => "pick",
        Cmd::Why { .. } => "why",
        Cmd::Run { .. } => "run",
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
            if format == Format::Human {
                if !out.human.is_empty() {
                    print!("{}", out.human);
                }
                if cli.g.verbose {
                    eprintln!("grevi: {}", out.data);
                    eprintln!(
                        "grevi: {} ms, {} requests, {} cached, ${:.5}",
                        meta.elapsed_ms, meta.requests, meta.cache_hits, meta.cost_usd
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
                println!("{}", output::render(format, &env).expect("render envelope"));
            }
            out.exit.code()
        }
        Err(e) => report_error(format, name, &e, meta),
    }
}

fn report_error(format: Format, name: &str, e: &GreviError, meta: Meta) -> i32 {
    if format == Format::Human {
        eprintln!(
            "grevi {name}: error: {e}\n  hint: {}\n  try:  {}",
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
        println!("{}", output::render(format, &env).expect("render envelope"));
    }
    e.exit().code()
}

/// Every verb is wired here once (Task 1). Later tasks replace stub bodies in `cmd/*.rs`
/// and never edit this function.
async fn dispatch(cli: &Cli, ctx: &config::Config) -> Result<cmd::Outcome, GreviError> {
    let machine = cli.g.format() != Format::Human;
    match &cli.cmd {
        Cmd::Pick { intent, top, index } => cmd::pick::run(ctx, intent, *top, *index).await,
        Cmd::Why {
            context,
            top,
            cmd: child,
        } => cmd::why::run(ctx, *context, *top, child).await,
        Cmd::Run {
            intent,
            yes,
            exec,
            dry_run,
            no_args,
        } => {
            cmd::run::run(
                ctx,
                &intent.join(" "),
                cmd::run::RunFlags {
                    yes: *yes,
                    exec: *exec,
                    dry_run: *dry_run,
                    no_args: *no_args,
                    machine,
                },
            )
            .await
        }
        Cmd::Is { condition, band } => cmd::is::run(ctx, condition, *band).await,
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

#[cfg(test)]
mod tests {
    use super::*;
    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn machine_format_is_read_from_raw_args_when_clap_fails() {
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
