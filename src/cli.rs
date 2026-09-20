use crate::output::Format;
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "grevi",
    version,
    about = "Answer questions about text you already have: find a line, an error, a command or a folder by meaning. grevi selects and never generates, with backend-specific decision scores.",
    after_help = "Examples:\n  gh run view --log-failed | grevi why\n  git branch | grevi pick \"the payment timeout fix\"\n  grevi is \"the customer asks for a refund\" < mail.txt && ./refund\n  grevi run --dry-run \"keep my mac awake for an hour\"\n  grevi add --dry-run \"the token expiry fix\"\n  grevi sort ~/Downloads\n\nExit codes: 0 ok, 1 no (is), 2 usage, 3 nothing fits or unsure, 4 API unavailable, 5 auth, 6 input, 7 child failed, 130 declined.\nAgents: grevi capabilities --json | grevi robot-docs"
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
    /// Decision threshold on the backend's score (calibration depends on task and backend)
    #[arg(short = 't', long, global = true, env = "GREVI_THRESHOLD")]
    pub threshold: Option<f64>,
    /// TypeSafe model or alias (default jev-1.13.0); unsupported by classifier.dev
    #[arg(long, global = true, env = "GREVI_MODEL")]
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
    /// Find one line in a list by describing it: stdin lines in, the matching line out
    #[command(
        after_help = "Examples:\n  git branch | grevi pick \"the payment timeout fix\"\n  git log --oneline | grevi pick -n 3 \"when we changed the pricing\"\n  code \"$(grevi pick --files . \"where man pages are parsed\")\"\n\nThe description and the line need no word in common. --files ranks the path names under DIR first, then reads the beginning of at most 24 finalist files; hidden files, git-ignored files and symlinks are skipped.\nExit: 0 found, 3 no line fits. --json data: matches[{line, text, p}], any, source."
    )]
    Pick {
        /// Describe the line you want, e.g. "the branch with the payment timeout fix"
        intent: String,
        /// Print up to N matches, each ranked above "nothing fits"
        #[arg(short = 'n', long, default_value_t = 1)]
        top: usize,
        /// Print 1-based line numbers instead of lines
        #[arg(long)]
        index: bool,
        /// Choose among the files under DIR instead of stdin lines; prints the path
        #[arg(long, value_name = "DIR")]
        files: Option<std::path::PathBuf>,
    },
    /// Find the line that caused a failure in build, test or CI output (stdin, or `-- <cmd>` to run it)
    #[command(
        after_help = "Examples:\n  cargo build 2>&1 | grevi why\n  gh run view --log-failed | grevi why --json\n  grevi why -- cargo test\n\nPipe 2>&1: compilers write errors to stderr. Works on logs of thousands of lines, and finds a cause that holds no word like \"error\".\nExit: 0 found, 3 no line looks like a failure. --json data: causes[{line, text, p, context[]}], any, considered, total, hint, child_exit."
    )]
    Why {
        /// Lines of context around the root cause
        #[arg(short = 'C', long, default_value_t = 3)]
        context: usize,
        /// Report up to N causes, each ranked above "no failure"
        #[arg(short = 'n', long, default_value_t = 1)]
        top: usize,
        /// Run this command and read its stdout+stderr instead of stdin: `grevi why -- cargo build`
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Describe a task in plain English and get a command proposal; only validated recipes can run
    #[command(
        after_help = "Examples:\n  grevi run --dry-run \"keep my mac awake for an hour\"\n  grevi run --json --dry-run --no-args \"test how fast my connection is\"\n\nSearches commands on PATH by their man pages. Flags form proposals to check yourself. Only exact zero-argument true, false, pwd, and ls recipes are complete and eligible to execute; all other argv have complete=false and a blocked reason. Human proposals use POSIX shell quoting.\nExit: 0 found (or ran), 3 no tool fits, 7 the command failed, 130 declined. --json data: tool, fit, argv[], flags[], complete, blocked, executed, child_exit, alternatives[]."
    )]
    Run {
        /// The task, e.g. "count the lines in notes.txt"; flags may follow it (`grevi run burn a dvd --dry-run`)
        #[arg(required = true, num_args = 1..)]
        intent: Vec<String>,
        /// Run a validated recipe without asking (only zero-argument true, false, pwd, ls)
        #[arg(short, long)]
        yes: bool,
        /// Allow validated recipes in machine mode (requires --yes)
        #[arg(long)]
        exec: bool,
        /// Only route and propose; never execute
        #[arg(long)]
        dry_run: bool,
        /// Route only; do not point at flags or files
        #[arg(long)]
        no_args: bool,
    },
    /// Ask a yes-or-no question about the text on stdin; the answer is the exit code (0 yes, 1 no, 3 unsure)
    #[command(
        after_help = "Examples:\n  grevi is \"the customer asks for a refund\" < mail.txt && ./refund\n  for f in mail/*; do grevi is \"asks for a refund\" < \"$f\"; echo \"$f $?\"; done\n\nWrite the statement literally: it is judged word for word. No counting, arithmetic, dates or quality judgments. Oversized input is not judged: no API call, exit 3, p=null, verdict=unsure, truncated=true, and a reason.\nPrints nothing on human stdout; oversized input warns on stderr. Exit: 0 yes, 1 no, 3 unsure. --json data: p, verdict, truncated, reason (when oversized)."
    )]
    Is {
        /// A statement that must be true of the text, e.g. "the customer asks for a refund"
        condition: String,
        /// Unsure band around the threshold (0..=0.5)
        #[arg(long, default_value_t = 0.15)]
        band: f64,
    },
    /// Stage only the git changes that belong to one topic, like `git add -p` without the questions
    #[command(
        after_help = "Examples:\n  grevi add --dry-run \"the token expiry fix\"\n  grevi add --yes \"the token expiry fix\" && git commit\n\nStages single hunks of tracked files, so it can split the changes of one file. Index only, never commits. Rejects hunks above 3000 characters and batches above the backend evidence budget before API requests or staging; no hunk evidence is clipped.\nExit: 0 staged (or scored with --dry-run), 3 no change is about the topic, 6 empty or oversized input, 130 declined. --json data: hunks[{file, header, p, staged}]."
    )]
    Add {
        /// The topic of the changes to stage, e.g. "the token expiry fix"
        topic: String,
        /// Stage without asking
        #[arg(short, long)]
        yes: bool,
        /// Score the hunks; stage nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Propose a folder for each file in a directory by reading the files; moves nothing without --apply
    #[command(
        after_help = "Examples:\n  grevi sort ~/Downloads\n  grevi sort ~/Downloads --apply\n  grevi sort ~/Downloads --undo <log>\n\nDestinations are the folders that already exist. Never overwrites, never deletes, same volume only.\nExit: 0 moves proposed (or applied), 3 nothing can be placed, 6 no folders to sort into. --json data: moves[{from, to, p}], skipped[{file, reason}], undo_log, applied."
    )]
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
    /// Check which backend answers, whether a key is needed, and how fast it replies
    Health,
    /// Print shell integration (`,` alias for `grevi run`)
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
    /// developer's exported `GREVI_THRESHOLD=abc` would fail an argv-only test: clear them first.
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
        assert_eq!(parse(&["grevi", "is", "x"]), Format::Human);
        assert_eq!(parse(&["grevi", "--robot", "is", "x"]), Format::Json);
        assert_eq!(
            parse(&["grevi", "--json", "--format", "toon", "is", "x"]),
            Format::Toon
        );
        // Global flags are accepted after the subcommand too.
        assert_eq!(
            parse(&["grevi", "is", "x", "--format", "jsonl"]),
            Format::Jsonl
        );
    }
}
