use std::fs::{self, DirBuilder, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Saves raw input; `None` skips saving. Call from the blocking pool.
/// Errors are reasons for the caller's `full output: not saved (<reason>)` line.
pub fn save(input: &[u8], directory: Option<&Path>) -> Result<PathBuf, String> {
    let directory =
        directory.ok_or_else(|| "saving disabled or directory unavailable".to_owned())?;
    let write = || -> io::Result<PathBuf> {
        let outputs = directory.join("outputs");
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&outputs)?;
        fs::set_permissions(&outputs, Permissions::from_mode(0o700))?;
        let hash = blake3::hash(input).to_hex();
        let path = outputs.join(format!("{}.log", &hash[..16]));
        // Exclusive creation avoids following an existing temporary file or sharing a writer.
        let (temporary, mut file) = loop {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let temporary = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)
            {
                Ok(file) => break (temporary, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        file.set_permissions(Permissions::from_mode(0o600))?;
        file.write_all(input)?;
        file.sync_all()?;
        drop(file);
        fs::rename(temporary, &path)?;
        Ok(path)
    };
    write().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> PathBuf {
        tempfile::Builder::new()
            .prefix("jevify-save-")
            .tempdir()
            .unwrap()
            .keep()
    }

    #[test]
    fn exact_bytes_hash_and_private_modes() {
        let directory = directory();
        let input = b"secret\0\xff\r\nlast line";
        let path = save(input, Some(&directory)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), input);
        let hash = blake3::hash(input).to_hex();
        assert_eq!(
            path,
            directory
                .join("outputs")
                .join(format!("{}.log", &hash[..16]))
        );
        assert_eq!(path.file_stem().unwrap().len(), 16);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[test]
    fn repeated_inputs_share_a_path_and_leave_no_temporary_files() {
        let directory = directory();
        let first = save(b"first", Some(&directory)).unwrap();
        assert_eq!(save(b"first", Some(&directory)).unwrap(), first);
        assert_eq!(fs::read_dir(directory.join("outputs")).unwrap().count(), 1);
        let second = save(b"second", Some(&directory)).unwrap();
        assert_ne!(first, second);
        let entries: Vec<_> = fs::read_dir(directory.join("outputs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&first));
        assert!(entries.contains(&second));
    }

    #[test]
    fn empty_input_is_saved() {
        let directory = directory();
        let path = save(b"", Some(&directory)).unwrap();
        assert!(fs::read(path).unwrap().is_empty());
    }

    #[test]
    fn skipped_save_returns_a_reason() {
        assert_eq!(
            save(b"input", None).unwrap_err(),
            "saving disabled or directory unavailable"
        );
    }

    #[test]
    fn unwritable_directory_returns_a_reason() {
        let directory = directory();
        fs::set_permissions(&directory, Permissions::from_mode(0o500)).unwrap();
        let result = save(b"input", Some(&directory));
        fs::set_permissions(&directory, Permissions::from_mode(0o700)).unwrap();
        assert!(!result.unwrap_err().is_empty());
    }

    #[test]
    fn directory_that_is_a_file_returns_a_reason() {
        let directory = directory();
        let file = directory.join("file");
        fs::write(&file, b"keep").unwrap();
        assert!(!save(b"input", Some(&file)).unwrap_err().is_empty());
        assert_eq!(fs::read(file).unwrap(), b"keep");
    }
}
