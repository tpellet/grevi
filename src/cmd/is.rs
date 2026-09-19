use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::HunchError;

/// Task 1 stub with the final signature; Task 5 replaces the body.
pub async fn run(ctx: &Config, condition: &str, band: f64) -> Result<Outcome, HunchError> {
    let _ = (ctx, condition, band);
    Err(HunchError::Usage("not implemented yet".into()))
}
