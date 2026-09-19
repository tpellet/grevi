use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::HunchError;

/// Task 1 stub with the final signature; Task 14 (wave 2) replaces the body.
pub async fn run(
    ctx: &Config,
    topic: &str,
    yes: bool,
    dry_run: bool,
    machine: bool,
) -> Result<Outcome, HunchError> {
    let _ = (ctx, topic, yes, dry_run, machine);
    Err(HunchError::Usage("not implemented yet".into()))
}
