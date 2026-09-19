use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::HunchError;
use std::path::Path;

/// Task 1 stub with the final signature; Task 15 (wave 2) replaces the body.
pub async fn run(
    ctx: &Config,
    dir: &Path,
    into: Option<&Path>,
    apply: bool,
    undo: Option<&Path>,
) -> Result<Outcome, HunchError> {
    let _ = (ctx, dir, into, apply, undo);
    Err(HunchError::Usage("not implemented yet".into()))
}
