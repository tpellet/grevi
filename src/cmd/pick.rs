use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::HunchError;

/// Task 1 stub with the final signature; Task 6 replaces the body.
pub async fn run(
    ctx: &Config,
    intent: &str,
    top: usize,
    index: bool,
) -> Result<Outcome, HunchError> {
    let _ = (ctx, intent, top, index);
    Err(HunchError::Usage("not implemented yet".into()))
}
