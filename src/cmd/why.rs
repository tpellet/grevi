use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::HunchError;

/// Task 1 stub with the final signature; Task 7 replaces the body.
pub async fn run(
    ctx: &Config,
    context: usize,
    top: usize,
    cmd: &[String],
) -> Result<Outcome, HunchError> {
    let _ = (ctx, context, top, cmd);
    Err(HunchError::Usage("not implemented yet".into()))
}
