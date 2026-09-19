use crate::cli::GlobalOpts;
use crate::exit::HunchError;
use crate::output::Meta;

/// Task 1 stub; Task 2 replaces this file.
pub struct Config;

impl Config {
    pub fn load(g: &GlobalOpts) -> Result<Self, HunchError> {
        let threshold = g.threshold.unwrap_or(0.5);
        if !(0.0..=1.0).contains(&threshold) {
            return Err(HunchError::Usage(format!(
                "threshold {threshold} must be within 0..=1"
            )));
        }
        Ok(Config)
    }
    pub fn meta(&self) -> Meta {
        Meta::default()
    }
}
