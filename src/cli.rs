use crate::output::Format;
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "hunch",
    version,
    about = "Point at the right thing among real things — by meaning, with calibrated confidence.",
    after_help = "Examples:\n  ls ~/Downloads | hunch pick \"last month's electricity bill\"\n  cargo build 2>&1 | hunch why\n  hunch why -- cargo build\n  hunch run \"burn a dvd from this iso\"\n  hunch is \"asks for a refund\" < mail.txt && ./refund\n\nAgents: hunch capabilities --json | hunch robot-docs"
)]
pub struct Cli {
    #[command(flatten)]
    pub g: GlobalOpts,
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Args, Debug, Clone)]
pub struct GlobalOpts {
    /// Machine output: one JSON envelope on stdout (alias: --robot)
    #[arg(long, global = true, alias = "robot")]
    pub json: bool,
    /// Output format (overrides --json)
    #[arg(long, global = true, value_enum)]
    pub format: Option<Format>,
    /// Decision threshold on calibrated probability
    #[arg(short = 't', long, global = true, env = "HUNCH_THRESHOLD")]
    pub threshold: Option<f64>,
    /// TypeSafe model or alias (default jev-1.13.0; `jev-latest` moves with each release)
    #[arg(long, global = true, env = "HUNCH_MODEL")]
    pub model: Option<String>,
    /// Skip the local answer cache
    #[arg(long, global = true)]
    pub no_cache: bool,
    /// Print probabilities and timing on stderr
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

impl GlobalOpts {
    pub fn format(&self) -> Format {
        self.format.unwrap_or(if self.json {
            Format::Json
        } else {
            Format::Human
        })
    }
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Print the stdin line(s) that match an intent (exit 3 if none fits)
    Pick {
        intent: String,
        /// Print up to N matches, each ranked above "nothing fits"
        #[arg(short = 'n', long, default_value_t = 1)]
        top: usize,
        /// Print 1-based line numbers instead of lines
        #[arg(long)]
        index: bool,
    },
    /// Point at the root-cause line in failing output (stdin, or `-- <cmd>` to run and capture it)
    Why {
        /// Lines of context around the root cause
        #[arg(short = 'C', long, default_value_t = 3)]
        context: usize,
        /// Report up to N causes, each ranked above "no failure"
        #[arg(short = 'n', long, default_value_t = 1)]
        top: usize,
        /// Run this command and read its stdout+stderr instead of stdin: `hunch why -- cargo build`
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Route an intent to an installed tool, point at flags from its man page, run on confirm
    Run {
        /// The request; flags may follow it (`hunch run burn a dvd --dry-run`)
        #[arg(required = true, num_args = 1..)]
        intent: Vec<String>,
        /// Run without asking
        #[arg(short, long)]
        yes: bool,
        /// Allow execution in machine mode (requires --yes)
        #[arg(long)]
        exec: bool,
        /// Only route and propose; never execute
        #[arg(long)]
        dry_run: bool,
        /// Route only; do not point at flags or files
        #[arg(long)]
        no_args: bool,
    },
    /// Exit 0 if stdin satisfies the condition, 1 if not, 3 if unsure
    Is {
        condition: String,
        /// Unsure band around the threshold (0..=0.5)
        #[arg(long, default_value_t = 0.15)]
        band: f64,
    },
    /// Stage only the git hunks about a topic (wave 2; hidden until Task 14 lands)
    #[command(hide = true)]
    Add {
        topic: String,
        /// Stage without asking
        #[arg(short, long)]
        yes: bool,
        /// Score the hunks; stage nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Propose moving files into existing folders by meaning (wave 2; hidden until Task 15 lands)
    #[command(hide = true)]
    Sort {
        /// Directory whose files (not recursive, not hidden) are sorted
        dir: std::path::PathBuf,
        /// Root whose sub-folders (depth <= 2) are the destinations (default: <DIR>)
        #[allow(rustdoc::invalid_html_tags)]
        #[arg(long)]
        into: Option<std::path::PathBuf>,
        /// Move the files (dry-run otherwise) and write an undo log
        #[arg(long)]
        apply: bool,
        /// Move files back using a log written by --apply
        #[arg(long)]
        undo: Option<std::path::PathBuf>,
    },
    /// Describe commands, flags, exit codes, env and limits for agents
    Capabilities,
    /// Agent handbook: guide | commands | exit-codes | examples | privacy
    RobotDocs { topic: Option<String> },
    /// Check the API key and TypeSafe reachability
    Health,
    /// Print shell integration (`,` alias for `hunch run`)
    Init { shell: Shell },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Shell {
    Zsh,
    Bash,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, FromArgMatches};

    /// `try_parse_from` still reads the `env = ".."` fallbacks from the real process, so a
    /// developer's exported `HUNCH_THRESHOLD=abc` would fail an argv-only test: clear them first.
    fn parse_without_env(args: &[&str]) -> Cli {
        let m = Cli::command()
            .mut_args(|a| a.env(None))
            .try_get_matches_from(args)
            .unwrap();
        Cli::from_arg_matches(&m).unwrap()
    }

    #[test]
    fn format_flag_overrides_json_and_robot_is_an_alias() {
        let parse = |a: &[&str]| parse_without_env(a).g.format();
        assert_eq!(parse(&["hunch", "is", "x"]), Format::Human);
        assert_eq!(parse(&["hunch", "--robot", "is", "x"]), Format::Json);
        assert_eq!(
            parse(&["hunch", "--json", "--format", "toon", "is", "x"]),
            Format::Toon
        );
        // Global flags are accepted after the subcommand too.
        assert_eq!(
            parse(&["hunch", "is", "x", "--format", "jsonl"]),
            Format::Jsonl
        );
    }
}
