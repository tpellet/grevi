use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::HunchError;

pub struct RunFlags {
    pub yes: bool,
    pub exec: bool,
    pub dry_run: bool,
    pub no_args: bool,
    pub machine: bool,
}

/// Task 1 stub with the final signature; Task 9 replaces the body (Task 10 extends it).
pub async fn run(ctx: &Config, intent: &str, flags: RunFlags) -> Result<Outcome, HunchError> {
    let _ = (
        ctx,
        intent,
        flags.yes,
        flags.exec,
        flags.dry_run,
        flags.no_args,
        flags.machine,
    );
    Err(HunchError::Usage("not implemented yet".into()))
}
