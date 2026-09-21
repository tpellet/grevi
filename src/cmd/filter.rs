use crate::{cmd::Outcome, config::Config, exit::JevifyError, records::Split};

pub struct FilterFlags {
    pub invert: bool,
    pub count: bool,
    pub strict: bool,
    pub split: Split,
    pub files: bool,
    pub no_save: bool,
}

pub async fn run(
    _ctx: &Config,
    _statement: &str,
    _flags: FilterFlags,
    _machine: bool,
) -> Result<Outcome, JevifyError> {
    Err(JevifyError::Input("filter: not implemented".into()))
}
