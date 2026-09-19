use crate::cli::Shell;
use crate::cmd::Outcome;
use crate::config::Config;
use crate::exit::{Exit, HunchError};

/// Task 1 stubs with the final signatures; Task 10 fills `init`, Task 11 the rest.
fn stub() -> Outcome {
    Outcome {
        exit: Exit::Usage,
        data: serde_json::Value::Null,
        human: String::new(),
    }
}
pub fn capabilities() -> Outcome {
    stub()
}
pub fn robot_docs(topic: Option<&str>) -> Result<Outcome, HunchError> {
    let _ = topic;
    Err(HunchError::Usage("not implemented yet".into()))
}
pub async fn health(ctx: &Config) -> Result<Outcome, HunchError> {
    let _ = ctx;
    Err(HunchError::Usage("not implemented yet".into()))
}
pub fn init(shell: Shell) -> Outcome {
    let _ = shell;
    stub()
}
