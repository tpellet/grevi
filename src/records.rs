use crate::exit::JevifyError;
use std::{ffi::OsString, ops::Range, path::Path};

pub struct Record {
    pub handle: OsString,
    pub evidence: String,
    pub raw: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    Lines,
    Nul,
    Para,
}

pub fn parse(_input: &[u8], _split: Split) -> Result<Vec<Record>, JevifyError> {
    Err(JevifyError::Input("byte records: not implemented".into()))
}

// Withhold all excerpts until the shared path policy is implemented.
pub fn withheld(_path: &Path) -> bool {
    true
}

pub async fn excerpt(path: &Path) -> Result<Option<String>, JevifyError> {
    if withheld(path) {
        return Ok(None);
    }
    Err(JevifyError::Input("file excerpts: not implemented".into()))
}
