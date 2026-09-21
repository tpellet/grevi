use crate::output::Format;
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "jevify",
    version,
    about = "Answer questions about text you already have: find a line, an error, a command or a folder by meaning. jevify selects and never generates, with backend-specific decision scores.",
    after_help = "Examples:\n  gh run view --log-failed | jevify why\n  git branch | jevify pick \"the payment timeout fix\"\n  cargo test 2>&1 | jevify filter \"reports a failed assertion\"\n  jevify is \"the customer asks for a refund\" < mail.txt && ./refund\n  jevify route \"keep my mac awake for an hour\"\n  jevify add --dry-run \"the token expiry fix\"\n  jevify sort ~/Downloads\n\nExit codes: 0 ok, 1 no, 2 usage, 3 nothing fits or unsure, 4 API unavailable, 5 auth, 6 input, 7 reserved, 130 declined.\nAgents: jevify capabilities --json | jevify init agents"
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
    #[arg(short = 't', long, global = true, env = "JEVIFY_THRESHOLD")]
    pub threshold: Option<f64>,
    /// TypeSafe model or alias (default jev-1.13.0); unsupported by classifier.dev
    #[arg(long, global = true, env = "JEVIFY_MODEL")]
    pub model: Option<String>,
    /// Skip the local answer cache
    #[arg(long, global = true)]
    pub no_cache: bool,
    /// Print probabilities and timing on stderr
    #[arg(long, global = true)]
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
        after_help = "Examples:\n  git branch | jevify pick \"the payment timeout fix\"\n  git log --oneline | jevify pick -n 3 \"when we changed the pricing\"\n  git ls-files | jevify pick --files \"where man pages are parsed\"\n\nThe description and the record need no word in common. --files ranks stdin paths first, then reads excerpts of the finalists; hidden or secret-looking paths and symlink files receive no excerpt (stderr: excerpts withheld: N). Selected records keep their bytes and input order. Input is not saved.\nExit: 0 found, 3 no record fits. --json data: matches[{line, text, ordinal, p, lossy?}], any, source. Non-UTF-8 records have lossy: true."
    )]
    Pick {
        /// Describe the line you want, e.g. "the branch with the payment timeout fix"
        intent: String,
        /// Print up to N matches, each ranked above "nothing fits"
        #[arg(short = 'n', long, default_value_t = 1)]
        top: usize,
        /// Print 1-based line numbers instead of lines
        #[arg(long, conflicts_with = "files")]
        index: bool,
        /// Read paths from stdin and use file excerpts as evidence
        #[arg(long)]
        files: bool,
        /// Split stdin on NUL bytes
        #[arg(short = '0', conflicts_with = "para")]
        nul: bool,
        /// Split stdin into paragraphs
        #[arg(long)]
        para: bool,
    },
    /// Find the line that caused a failure in build, test or CI output on stdin
    #[command(
        after_help = "Examples:\n  cargo build 2>&1 | jevify why\n  gh run view --log-failed | jevify why --json\n\nPipe 2>&1: compilers write errors to stderr. Prints numbered context; takes no split option. Saves raw input, secrets included, unless --no-save; stderr names the full output path.\nExit: 0 found, 3 no line looks like a failure. --json data: causes[{line, text, p, context[]}], any, considered, total, hint, saved_input, complete. A skipped or failed save sets complete: false."
    )]
    Why {
        /// Lines of context around the root cause
        #[arg(short = 'C', long, default_value_t = 3)]
        context: usize,
        /// Report up to N causes, each ranked above "no failure"
        #[arg(short = 'n', long, default_value_t = 1)]
        top: usize,
        /// Do not save the full input
        #[arg(long)]
        no_save: bool,
    },
    /// Describe a task and print the installed tool that fits it
    #[command(
        after_help = "Examples:\n  jevify route \"keep my mac awake for an hour\"\n  jevify route --json \"test how fast my connection is\"\n\nSearches commands on PATH by their man pages and prints a tool, summary and synopsis. Starts no command.\nExit: 0 found, 3 no tool fits. --json data: tool, summary, synopsis, fit, alternatives[]."
    )]
    Route {
        /// The task, e.g. "count the lines in notes.txt"
        #[arg(required = true, num_args = 1..)]
        intent: Vec<String>,
    },
    /// Keep stdin records that satisfy a statement
    #[command(
        after_help = "Example:\n  cargo test 2>&1 | jevify filter 'reports a failed assertion'\n\n-v inverts; -c prints the count. Unsure records stay unless --strict. --verbose has no short flag. --files reads stdin paths; hidden or secret-looking paths and symlink files receive no excerpt (excerpts withheld: N). Saves raw input, secrets included, unless --no-save. Status: jevify filter: kept N of M, U unsure, full output: PATH.\nExit: 0 kept some, 1 kept none, 3 every record unsure. --json data: records[{text, ordinal, p, verdict, lossy?}], kept, total, unsure, complete, saved_input, excerpts_withheld. A skipped or failed save sets complete: false. Non-UTF-8 records have lossy: true."
    )]
    Filter {
        statement: String,
        #[arg(short = 'v')]
        invert: bool,
        #[arg(short = 'c')]
        count: bool,
        #[arg(long)]
        strict: bool,
        #[arg(short = '0', conflicts_with = "para")]
        nul: bool,
        #[arg(long)]
        para: bool,
        #[arg(long)]
        files: bool,
        #[arg(long)]
        no_save: bool,
    },
    /// Ask a yes-or-no question about the text on stdin; the answer is the exit code (0 yes, 1 no, 3 unsure)
    #[command(
        after_help = "Examples:\n  jevify is \"the customer asks for a refund\" < mail.txt && ./refund\n  jevify is 'asks for a refund' 'mentions an order' --context mail.txt\n\nWrite the condition so that yes means act. Each statement is judged literally. No counting, arithmetic, dates or quality judgments. Oversized input is not judged.\nOne statement prints nothing on human stdout; several print VERDICT<TAB>STATEMENT lines. Exit: 0 all yes, 1 one no, 3 otherwise. --json data: p, verdict, truncated, reason (when oversized); several: statements[{statement, verdict, p}], verdict, truncated."
    )]
    Is {
        /// A statement that must be true of the text, e.g. "the customer asks for a refund"
        #[arg(required = true, num_args = 1..)]
        statements: Vec<String>,
        /// Read the context from a file instead of stdin
        #[arg(long, value_name = "FILE")]
        context: Option<std::path::PathBuf>,
        /// Unsure band around the threshold (0..=0.5)
        #[arg(long, default_value_t = 0.15)]
        band: f64,
    },
    /// Stage only the git changes that belong to one topic, like `git add -p` without the questions
    #[command(
        after_help = "Examples:\n  jevify add --dry-run \"the token expiry fix\"\n  jevify add --yes \"the token expiry fix\" && git commit\n\nStages single hunks of tracked files, so it can split the changes of one file. Index only, never commits. Rejects hunks above 3000 characters and batches above the backend evidence budget before API requests or staging; no hunk evidence is clipped.\nExit: 0 staged (or scored with --dry-run), 3 no change is about the topic, 6 empty or oversized input, 130 declined. --json data: hunks[{file, header, p, staged}]."
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
        after_help = "Examples:\n  jevify sort ~/Downloads\n  jevify sort ~/Downloads --apply\n  jevify sort ~/Downloads --undo <log>\n\nDestinations are the folders that already exist. Never overwrites, never deletes, same volume only.\nExit: 0 moves proposed (or applied), 3 nothing can be placed, 6 no folders to sort into. --json data: moves[{from, to, p}], skipped[{file, reason}], undo_log, applied."
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
    /// Print shell integration (`,` alias for `jevify route`) or an agent instruction block
    Init { shell: Shell },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Shell {
    Zsh,
    Bash,
    Agents,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, FromArgMatches};

    /// `try_parse_from` still reads the `env = ".."` fallbacks from the real process, so a
    /// developer's exported `JEVIFY_THRESHOLD=abc` would fail an argv-only test: clear them first.
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
        assert_eq!(parse(&["jevify", "is", "x"]), Format::Human);
        assert_eq!(parse(&["jevify", "--robot", "is", "x"]), Format::Json);
        assert_eq!(
            parse(&["jevify", "--json", "--format", "toon", "is", "x"]),
            Format::Toon
        );
        // Global flags are accepted after the subcommand too.
        assert_eq!(
            parse(&["jevify", "is", "x", "--format", "jsonl"]),
            Format::Jsonl
        );
    }

    #[test]
    fn output_verb_flags_parse_without_stealing_text() {
        let cli = parse_without_env(&[
            "jevify",
            "filter",
            "-v",
            "-c",
            "--strict",
            "-0",
            "--files",
            "--no-save",
            "--verbose",
            "--",
            "-statement",
        ]);
        assert!(cli.g.verbose);
        assert!(
            matches!(cli.cmd, Cmd::Filter { statement, invert: true, count: true, strict: true, nul: true, para: false, files: true, no_save: true } if statement == "-statement")
        );
        assert!(
            matches!(parse_without_env(&["jevify", "is", "a", "b", "--context", "FILE"]).cmd, Cmd::Is { statements, context: Some(path), .. } if statements == ["a", "b"] && path == std::path::Path::new("FILE"))
        );
        assert!(
            matches!(parse_without_env(&["jevify", "pick", "--files", "--para", "--", "-query"]).cmd, Cmd::Pick { intent, files: true, para: true, .. } if intent == "-query")
        );
        assert!(matches!(
            parse_without_env(&["jevify", "why", "--no-save", "-C", "2", "-n", "3"]).cmd,
            Cmd::Why {
                context: 2,
                top: 3,
                no_save: true
            }
        ));
        assert!(matches!(
            parse_without_env(&["jevify", "init", "agents"]).cmd,
            Cmd::Init {
                shell: Shell::Agents
            }
        ));
    }

    #[test]
    fn invalid_output_verb_flag_combinations_are_usage_errors() {
        for args in [
            vec!["jevify", "is", "x", "-v"],
            vec!["jevify", "pick", "-0", "--para", "q"],
            vec!["jevify", "filter", "-0", "--para", "q"],
            vec!["jevify", "why", "-0"],
            vec!["jevify", "why", "--para"],
            vec!["jevify", "why", "--files"],
            vec!["jevify", "pick", "--files", "DIR", "q"],
            vec!["jevify", "pick", "--files", "--index", "q"],
        ] {
            let error = Cli::command()
                .mut_args(|a| a.env(None))
                .try_get_matches_from(&args)
                .unwrap_err();
            assert_eq!(error.exit_code(), 2, "{args:?}");
        }
    }
}
