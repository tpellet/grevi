use std::{ffi::OsString, ops::Range};

#[derive(Debug, Clone)]
pub struct Arg {
    pub literal: OsString,
    pub markers: Vec<Marker>,
}

#[derive(Debug, Clone)]
pub struct Marker {
    pub kind: String,
    pub description: String,
    pub span: Range<usize>,
}

#[derive(Debug, thiserror::Error)]
#[error("marker lexer is not implemented")]
pub struct MarkerError;

pub fn parse(_argv: &[OsString]) -> Result<Vec<Arg>, MarkerError> {
    Err(MarkerError)
}

pub fn substitute(_args: &[Arg], _handles: &[OsString]) -> Result<Vec<OsString>, MarkerError> {
    Err(MarkerError)
}
