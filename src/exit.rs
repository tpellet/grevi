use serde::Serialize;

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
        (Exit::Ok, "success: yes / found / executed"),
        (Exit::No, "`is`: the condition does not hold"),
        (Exit::Usage, "usage error: bad flag or missing argument"),
        (Exit::Abstain, "abstain: nothing fits, or unsure"),
        (Exit::Unavailable, "TypeSafe API unavailable after retries"),
        (Exit::Auth, "API key missing or rejected"),
        (Exit::Input, "input error: empty, too large, or unreadable"),
        (Exit::ChildFailed, "`run`: the executed command failed"),
        (Exit::Interrupted, "interrupted or declined at confirmation"),
    ];
}

#[derive(Debug, thiserror::Error)]
pub enum GreviError {
    #[error("no TypeSafe API key: set TYPESAFE_API_KEY or TYPESAFE_API_KEY_FILE")]
    MissingKey,
    #[error("TypeSafe rejected the API key (HTTP {0})")]
    BadKey(u16),
    #[error("TypeSafe API unavailable: {0}")]
    Unavailable(String),
    #[error("unexpected response from TypeSafe: {0}")]
    Protocol(String),
    #[error("no input: {0}")]
    EmptyInput(&'static str),
    #[error("input too large: {0}")]
    InputTooLarge(String),
    /// HTTP 413/422: the API rejected the request body. Usually the state is over the token
    /// budget, sometimes the request is malformed (a grevi bug); an input error, not an outage.
    #[error("TypeSafe rejected the request (HTTP {0}): {1}")]
    RejectedRequest(u16, String),
    #[error("{0}")]
    Input(String),
    #[error("{0}")]
    Usage(String),
    #[error("declined")]
    Declined,
}

impl GreviError {
    pub fn exit(&self) -> Exit {
        match self {
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
            Self::MissingKey => {
                "create a key at https://console.typesafe.ai/settings/keys and export it in your shell profile; grevi never prints it"
            }
            Self::BadKey(_) => "check the key in the TypeSafe console; `grevi health` verifies it",
            Self::Unavailable(_) => "retry later, or lower GREVI_CONCURRENCY if rate limited",
            Self::Protocol(_) => {
                "the TypeSafe API may have changed, or GREVI_BASE_URL points at the wrong server; run `grevi health` and report the issue with `grevi --version`"
            }
            Self::EmptyInput(msg) if msg.starts_with("no unstaged changes") => {
                "nothing to stage: `git diff` is empty (untracked files are never staged by add)"
            }
            Self::EmptyInput(_) => "pipe text into grevi",
            Self::InputTooLarge(_) => "filter the input first, e.g. with rg or tail",
            Self::RejectedRequest(..) => {
                "the input is probably over the API's token budget: filter it first, e.g. with rg or tail; if it is small, this is a grevi bug — report it with `grevi --version`"
            }
            Self::Input(_) => "check the input path and encoding",
            Self::Usage(_) => "see `grevi --help` or `grevi capabilities --json`",
            Self::Declined => "re-run with --yes to skip confirmation",
        }
    }
    pub fn example(&self) -> &'static str {
        match self {
            Self::MissingKey => "export TYPESAFE_API_KEY=...; grevi health",
            Self::EmptyInput(msg) if msg.starts_with("no unstaged changes") => {
                "grevi add \"finish the login flow\""
            }
            Self::EmptyInput(_) => "ls | grevi pick \"the invoice from March\"",
            Self::InputTooLarge(_) | Self::RejectedRequest(..) => {
                "tail -n 20000 build.log | grevi why"
            }
            _ => "grevi capabilities --json",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_codes_are_the_documented_contract() {
        let codes: Vec<i32> = Exit::ALL.iter().map(|(e, _)| e.code()).collect();
        assert_eq!(codes, [0, 1, 2, 3, 4, 5, 6, 7, 130]);
    }
    #[test]
    fn every_error_maps_to_a_stable_kind_and_exit() {
        let errors = [
            GreviError::MissingKey,
            GreviError::BadKey(401),
            GreviError::Unavailable(String::new()),
            GreviError::Protocol(String::new()),
            GreviError::EmptyInput(""),
            GreviError::InputTooLarge(String::new()),
            GreviError::RejectedRequest(422, String::new()),
            GreviError::Input(String::new()),
            GreviError::Usage(String::new()),
            GreviError::Declined,
        ];
        let expected = [
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
            assert!(!e.hint().is_empty() && e.example().contains("grevi"));
        }
    }
}
