use crate::{cmd::Outcome, config::Config, exit::JevifyError, records::Split};
use std::{ffi::OsString, path::PathBuf};

pub struct FillFlags {
    pub dry_run: bool,
    pub quiet: bool,
    pub candidates: Option<PathBuf>,
    pub context: Option<PathBuf>,
    pub field: Option<usize>,
    pub key: Option<String>,
    pub split: Split,
}

pub async fn run(
    _ctx: &Config,
    flags: FillFlags,
    _cmd: &[OsString],
    machine: bool,
) -> Result<Outcome, JevifyError> {
    if machine && !flags.dry_run {
        return Err(JevifyError::Usage(
            "machine output requires --dry-run".into(),
        ));
    }
    Err(JevifyError::Input("fill is not implemented".into()))
}
