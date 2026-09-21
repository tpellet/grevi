use serde::Serialize;

pub const NO_MATCH: &str = "no_match";
pub const AMBIGUOUS: &str = "ambiguous";
pub const UNSURE_FLAG: &str = "unsure_flag";
pub const INSUFFICIENT_EVIDENCE: &str = "insufficient_evidence";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Exit {
    Ok = 0,
    No = 1,
    Usage = 2,
    Abstain = 3,
    Unavailable = 4,
    Auth = 5,
    Input = 6,
    ChildFailed = 7,
    Interrupted = 130,
}

impl Exit {
    pub fn code(self) -> i32 {
        self as i32
    }
    pub const ALL: [(Exit, &'static str); 9] = [
        (Exit::Ok, "success: yes / found"),
        (Exit::No, "`is`: the condition does not hold"),
        (Exit::Usage, "usage error: bad flag or missing argument"),
        (Exit::Abstain, "abstain: nothing fits, or unsure"),
        (Exit::Unavailable, "the API is unavailable after retries"),
        (Exit::Auth, "API key missing or rejected"),
        (Exit::Input, "input error: empty, too large, or unreadable"),
        (Exit::ChildFailed, "reserved"),
        (Exit::Interrupted, "interrupted or declined at confirmation"),
    ];
}

#[derive(Debug, thiserror::Error)]
pub enum JevifyError {
    #[error("{message}")]
    Kinded {
        kind: &'static str,
        exit: Exit,
        message: String,
        hint: &'static str,
        example: &'static str,
    },
    /// Only reachable with `JEVIFY_BACKEND=typesafe`: without a key jevify uses classifier.dev.
    #[error("the typesafe backend needs a key: set TYPESAFE_API_KEY or TYPESAFE_API_KEY_FILE")]
    MissingKey,
    #[error("the API rejected the key (HTTP {0})")]
    BadKey(u16),
    #[error("API unavailable: {0}")]
    Unavailable(String),
    #[error("unexpected response from the API: {0}")]
    Protocol(String),
    #[error("no input: {0}")]
    EmptyInput(&'static str),
    #[error("input too large: {0}")]
    InputTooLarge(String),
    /// HTTP 413/422: the API rejected the request body. Usually the state is over the token
    /// budget, sometimes the request is malformed (a jevify bug); an input error, not an outage.
    #[error("the API rejected the request (HTTP {0}): {1}")]
    RejectedRequest(u16, String),
    #[error("{0}")]
    Input(String),
    #[error("{0}")]
    Usage(String),
    #[error("declined")]
    Declined,
}

impl JevifyError {
    pub fn stdin_is_tty(message: String) -> Self {
        Self::Kinded {
            kind: "stdin_is_tty",
            exit: Exit::Input,
            message,
            hint: "pipe candidates or context, or provide a file",
            example: "jevify fill --candidates input -- CMD '@{-:description}'",
        }
    }
    pub fn lister_failed(message: String) -> Self {
        Self::Kinded {
            kind: "lister_failed",
            exit: Exit::Input,
            message,
            hint: "check the lister and narrow its scope",
            example: "jevify pick --from branch 'description'",
        }
    }
    pub fn cannot_run(message: String) -> Self {
        Self::Kinded {
            kind: "cannot_run",
            exit: Exit::Input,
            message,
            hint: "check the command path and executable permissions",
            example: "jevify fill --dry-run -- CMD '@{-:description}'",
        }
    }
    pub fn recipe_invalid(message: String) -> Self {
        Self::Kinded {
            kind: "recipe_invalid",
            exit: Exit::Input,
            message,
            hint: "correct the kind recipe",
            example: "jevify capabilities --json",
        }
    }
    pub fn exit(&self) -> Exit {
        match self {
            Self::Kinded { exit, .. } => *exit,
            Self::MissingKey | Self::BadKey(_) => Exit::Auth,
            Self::Unavailable(_) | Self::Protocol(_) => Exit::Unavailable,
            Self::EmptyInput(_)
            | Self::InputTooLarge(_)
            | Self::RejectedRequest(..)
            | Self::Input(_) => Exit::Input,
            Self::Usage(_) => Exit::Usage,
            Self::Declined => Exit::Interrupted,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Kinded { kind, .. } => kind,
            Self::MissingKey => "missing_api_key",
            Self::BadKey(_) => "bad_api_key",
            Self::Unavailable(_) => "api_unavailable",
            Self::Protocol(_) => "api_protocol",
            Self::EmptyInput(_) => "empty_input",
            Self::InputTooLarge(_) => "input_too_large",
            Self::RejectedRequest(..) => "api_rejected_request",
            Self::Input(_) => "input",
            Self::Usage(_) => "usage",
            Self::Declined => "declined",
        }
    }
    pub fn hint(&self) -> &'static str {
        match self {
            Self::Kinded { hint, .. } => hint,
            Self::MissingKey => {
                "unset JEVIFY_BACKEND to run keyless through classifier.dev, or create a key at https://console.typesafe.ai/settings/keys and export it in your shell profile; jevify never prints it"
            }
            Self::BadKey(_) => "check the key in the TypeSafe console; `jevify health` verifies it",
            Self::Unavailable(_) => "retry later, or lower JEVIFY_CONCURRENCY if rate limited",
            Self::Protocol(_) => {
                "the API may have changed, or JEVIFY_BASE_URL points at the wrong server; run `jevify health` and report the issue with `jevify --version`"
            }
            Self::EmptyInput(msg) if msg.starts_with("no unstaged changes") => {
                "nothing to stage: `git diff` is empty (untracked files are never staged by add)"
            }
            Self::EmptyInput(_) => "pipe text into jevify",
            Self::InputTooLarge(_) => "filter the input first, e.g. with rg or tail",
            Self::RejectedRequest(..) => {
                "the input is probably over the API's token budget: filter it first, e.g. with rg or tail; if it is small, this is a jevify bug — report it with `jevify --version`"
            }
            Self::Input(_) => "check the input path and encoding",
            Self::Usage(_) => "see `jevify --help` or `jevify capabilities --json`",
            Self::Declined => "re-run with --yes to skip confirmation",
        }
    }
    pub fn example(&self) -> &'static str {
        match self {
            Self::Kinded { example, .. } => example,
            Self::MissingKey => "export TYPESAFE_API_KEY=...; jevify health",
            Self::EmptyInput(msg) if msg.starts_with("no unstaged changes") => {
                "jevify add \"finish the login flow\""
            }
            Self::EmptyInput(_) => "ls | jevify pick \"the invoice from March\"",
            Self::InputTooLarge(_) | Self::RejectedRequest(..) => {
                "tail -n 20000 build.log | jevify why"
            }
            _ => "jevify capabilities --json",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fill_kinds_and_abstention_reasons_are_stable() {
        use crate::output::{Envelope, ErrorBody, Meta};
        for (error, kind) in [
            (JevifyError::stdin_is_tty("terminal".into()), "stdin_is_tty"),
            (JevifyError::lister_failed("failed".into()), "lister_failed"),
            (JevifyError::cannot_run("missing".into()), "cannot_run"),
            (
                JevifyError::recipe_invalid("invalid".into()),
                "recipe_invalid",
            ),
        ] {
            let envelope = Envelope {
                ok: false,
                command: "fill",
                version: "test",
                exit_code: error.exit().code(),
                data: serde_json::Value::Null,
                meta: Meta::default(),
                error: Some(ErrorBody {
                    kind: error.kind(),
                    message: error.to_string(),
                    hint: error.hint(),
                    example: error.example(),
                }),
            };
            let value = serde_json::to_value(envelope).unwrap();
            assert_eq!(value["exit_code"], 6);
            assert_eq!(value["error"]["kind"], kind);
        }
        assert_eq!(
            [NO_MATCH, AMBIGUOUS, UNSURE_FLAG, INSUFFICIENT_EVIDENCE],
            [
                "no_match",
                "ambiguous",
                "unsure_flag",
                "insufficient_evidence"
            ]
        );
    }
    #[test]
    fn exit_codes_are_the_documented_contract() {
        let codes: Vec<i32> = Exit::ALL.iter().map(|(e, _)| e.code()).collect();
        assert_eq!(codes, [0, 1, 2, 3, 4, 5, 6, 7, 130]);
        assert!(
            Exit::ALL
                .iter()
                .all(|(_, text)| !text.contains("executed") && !text.contains("`run`"))
        );
    }
    #[test]
    fn every_error_maps_to_a_stable_kind_and_exit() {
        let errors = [
            JevifyError::Kinded {
                kind: "too_many",
                exit: Exit::Input,
                message: "too many records".into(),
                hint: "narrow the input",
                example: "head -n 100 input | jevify pick 'q'",
            },
            JevifyError::MissingKey,
            JevifyError::BadKey(401),
            JevifyError::Unavailable(String::new()),
            JevifyError::Protocol(String::new()),
            JevifyError::EmptyInput(""),
            JevifyError::InputTooLarge(String::new()),
            JevifyError::RejectedRequest(422, String::new()),
            JevifyError::Input(String::new()),
            JevifyError::Usage(String::new()),
            JevifyError::Declined,
        ];
        let expected = [
            ("too_many", 6),
            ("missing_api_key", 5),
            ("bad_api_key", 5),
            ("api_unavailable", 4),
            ("api_protocol", 4),
            ("empty_input", 6),
            ("input_too_large", 6),
            ("api_rejected_request", 6),
            ("input", 6),
            ("usage", 2),
            ("declined", 130),
        ];
        for (e, (kind, code)) in errors.iter().zip(expected) {
            assert_eq!((e.kind(), e.exit().code()), (kind, code), "{e}");
            assert!(!e.hint().is_empty() && e.example().contains("jevify"));
        }
    }
}
