//! `label a,b,c`: tag each stdin record with one of the caller's labels.
//!
//! Output is `LABEL<TAB>RECORD` in input order, `?` for an unsure record. The labels are
//! validated by the parser (`cli::Labels`); their count against the backend's window is
//! checked here, after `Config::load`, where the backend is known.

use crate::{
    cmd::Outcome,
    config::Config,
    exit::{Exit, JevifyError},
    records::Split,
};

pub struct LabelFlags {
    pub split: Split,
    pub files: bool,
}

pub async fn run(
    _ctx: &Config,
    labels: Vec<String>,
    _flags: LabelFlags,
    _machine: bool,
) -> Result<Outcome, JevifyError> {
    Err(JevifyError::Kinded {
        kind: "not_implemented",
        exit: Exit::Input,
        message: format!(
            "label is not implemented yet: {} labels given, no record judged",
            labels.len()
        ),
        hint: "tag records by hand, or keep matching ones with filter",
        example: "gh issue list | jevify filter 'reports a crash'",
    })
}
