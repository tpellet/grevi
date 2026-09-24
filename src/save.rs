use std::fs::{self, DirBuilder, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// How long a saved input stays on disk, matching the answer cache: raw input holds more than
/// an answer does, so it does not outlive one. A save refreshes its own file's age, so an input
/// that keeps arriving keeps its file.
pub const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Names the store writes: `<blake3-16>.log`, and the `<blake3-16>.tmp-<pid>-<n>` a crash
/// between creation and rename leaves behind. Anything else in the directory is somebody
/// else's file and is never touched.
fn prunable(name: &str) -> bool {
    let Some((stem, rest)) = name.split_once('.') else {
        return false;
    };
    stem.len() == 16
        && stem
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() && b.is_ascii_hexdigit())
        && (rest == "log" || rest.starts_with("tmp-"))
}

/// Deletes saved inputs older than `RETENTION`, and nothing else. Best effort: pruning never
/// fails a save.
///
/// The bound of the deletion is the one directory handle: entries come from reading `outputs`
/// itself, no descent and no path built from their contents. A symlink is left alone, both as
/// the directory (`outputs` replaced by a link elsewhere prunes nothing) and as an entry (only
/// a regular file is removed), so pruning cannot reach a file outside the store.
fn prune(outputs: &Path) {
    if !fs::symlink_metadata(outputs).is_ok_and(|m| m.file_type().is_dir()) {
        return;
    }
    let Ok(entries) = fs::read_dir(outputs) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        if !entry.file_name().to_str().is_some_and(prunable) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|age| age > RETENTION));
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

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
        // On write, never on read: a verb that only reads a saved input deletes nothing.
        prune(&outputs);
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

    /// Backdates a file by `RETENTION` plus an hour, so it is past retention whatever the clock.
    fn age(path: &Path) {
        let stale = std::time::SystemTime::now() - RETENTION - Duration::from_secs(3600);
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(stale))
            .unwrap();
    }

    #[test]
    fn a_save_prunes_saved_inputs_past_retention_and_keeps_the_rest() {
        let directory = directory();
        let stale = save(b"stale", Some(&directory)).unwrap();
        let fresh = save(b"fresh", Some(&directory)).unwrap();
        let outputs = directory.join("outputs");
        let temporary = outputs.join("0123456789abcdef.tmp-1-0");
        fs::write(&temporary, b"crashed").unwrap();
        age(&stale);
        age(&temporary);
        // A file the store did not write is not the store's to delete, however old.
        let stranger = outputs.join("notes.txt");
        fs::write(&stranger, b"keep").unwrap();
        age(&stranger);

        save(b"third", Some(&directory)).unwrap();

        assert!(!stale.exists(), "a saved input past retention is deleted");
        assert!(!temporary.exists(), "a stranded temporary is deleted");
        assert!(fresh.exists(), "a saved input within retention survives");
        assert_eq!(fs::read(&stranger).unwrap(), b"keep");
    }

    #[test]
    fn a_repeated_input_keeps_its_file_by_refreshing_its_age() {
        let directory = directory();
        let path = save(b"recurring", Some(&directory)).unwrap();
        age(&path);
        assert_eq!(save(b"recurring", Some(&directory)).unwrap(), path);
        assert_eq!(fs::read(&path).unwrap(), b"recurring");
    }

    #[test]
    fn pruning_never_leaves_the_outputs_directory() {
        let directory = directory();
        let outside = directory.join("outside.log");
        fs::write(&outside, b"not the store's file").unwrap();
        age(&outside);
        let elsewhere = directory.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        // A real saved-input name, aged past retention, but outside the store.
        let decoy = elsewhere.join("0123456789abcdef.log");
        fs::write(&decoy, b"not the store's file either").unwrap();
        age(&decoy);
        // Saving creates outputs/, then a link inside it points at the file outside.
        save(b"input", Some(&directory)).unwrap();
        let outputs = directory.join("outputs");
        std::os::unix::fs::symlink(&decoy, outputs.join("fedcba9876543210.log")).unwrap();
        let mut deep = outputs.join("nested");
        fs::create_dir(&deep).unwrap();
        deep.push("0123456789abcdef.log");
        fs::write(&deep, b"a subdirectory is not scanned").unwrap();
        age(&deep);

        save(b"another input", Some(&directory)).unwrap();

        assert_eq!(fs::read(&outside).unwrap(), b"not the store's file");
        assert_eq!(fs::read(&decoy).unwrap(), b"not the store's file either");
        assert_eq!(fs::read(&deep).unwrap(), b"a subdirectory is not scanned");
        assert!(fs::symlink_metadata(outputs.join("fedcba9876543210.log")).is_ok());
    }

    #[test]
    fn an_outputs_directory_replaced_by_a_symlink_prunes_nothing() {
        let directory = directory();
        let elsewhere = directory.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        let victim = elsewhere.join("0123456789abcdef.log");
        fs::write(&victim, b"someone else's file").unwrap();
        age(&victim);
        std::os::unix::fs::symlink(&elsewhere, directory.join("outputs")).unwrap();
        // The save itself follows the link, as any path does; the pruning does not run.
        save(b"input", Some(&directory)).unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"someone else's file");
    }

    #[test]
    fn only_the_stores_own_filenames_are_prunable() {
        for name in [
            "0123456789abcdef.log",
            "0123456789abcdef.tmp-4321-0",
            "aaaaaaaaaaaaaaaa.log",
        ] {
            assert!(prunable(name), "{name}");
        }
        for name in [
            "notes.txt",
            "0123456789abcdef.log.bak",
            "0123456789ABCDEF.log",
            "0123456789abcdez.log",
            "0123456789abcde.log",
            "0123456789abcdef0.log",
            "0123456789abcdef",
            ".log",
            "0123456789abcdef.tmp",
            "answers",
        ] {
            assert!(!prunable(name), "{name}");
        }
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
