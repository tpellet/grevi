use std::path::{Path, PathBuf};

pub async fn save(_input: &[u8], _directory: Option<&Path>) -> Result<PathBuf, String> {
    Err("saved input: not implemented".into())
}
