use super::Response;
use std::path::PathBuf;
use std::time::Duration;

/// Entries older than this are ignored: under `--model jev-latest` the alias moves while the
/// cache key stays the same, and the directory would otherwise grow forever.
const TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub fn key(body: &[u8]) -> String {
    blake3::hash(body).to_hex().to_string()
}

pub struct DiskCache {
    dir: PathBuf,
}

impl DiskCache {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }
    fn path(&self, key: &str) -> PathBuf {
        self.dir
            .join("answers")
            .join(&key[..2])
            .join(format!("{key}.json"))
    }
    pub fn get(&self, key: &str) -> Option<Response> {
        let path = self.path(key);
        let age = std::fs::metadata(&path)
            .ok()?
            .modified()
            .ok()?
            .elapsed()
            .ok()?;
        if age > TTL {
            return None;
        }
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }
    /// Best effort: a failed cache write must never fail the command.
    pub fn put(&self, key: &str, r: &Response) {
        let path = self.path(key);
        let Some(parent) = path.parent() else { return };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if let Ok(bytes) = serde_json::to_vec(r) {
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let c = DiskCache::new(dir.path().to_path_buf()).unwrap();
        let r: Response =
            serde_json::from_str(r#"{"model":"m","answers":{},"usage":{"input_tokens":1}}"#)
                .unwrap();
        let k = key(b"body");
        assert!(c.get(&k).is_none());
        c.put(&k, &r);
        assert_eq!(c.get(&k).unwrap().model, "m");
    }
}
