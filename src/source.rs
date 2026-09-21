use crate::{
    exit::JevifyError,
    records::{Record, Split},
};
use std::{
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};

pub const KINDS: &[&str] = &["-", "branch"];

pub enum Scope {
    Input {
        bytes: Vec<u8>,
        split: Split,
        field: Option<usize>,
        key: Option<String>,
    },
    Prefix(Option<PathBuf>),
}

pub struct Listing {
    pub records: Vec<Record>,
    pub total: usize,
    pub omitted: usize,
    pub ordered: bool,
}

pub struct Env {
    pub path: OsString,
    pub config_dir: Option<PathBuf>,
    pub deadline: Instant,
}

impl Env {
    pub fn from_process(timeout: Duration) -> Self {
        let vars: std::collections::HashMap<_, _> = std::env::vars_os().collect();
        Self {
            path: vars
                .get(std::ffi::OsStr::new("PATH"))
                .cloned()
                .unwrap_or_default(),
            config_dir: vars
                .get(std::ffi::OsStr::new("JEVIFY_CONFIG_DIR"))
                .map(PathBuf::from),
            deadline: Instant::now() + timeout,
        }
    }
}

pub async fn enumerate(
    _kind: &str,
    _scope: Scope,
    _limit: usize,
    _env: &Env,
) -> Result<Listing, JevifyError> {
    Err(JevifyError::Input(
        "source enumeration is not implemented".into(),
    ))
}

pub async fn enrich(_kind: &str, handles: &[OsString]) -> Vec<String> {
    handles.iter().map(|_| String::new()).collect()
}
