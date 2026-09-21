use crate::exit::JevifyError;
use std::collections::HashMap;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::{ffi::OsString, ops::Range, path::Path};

#[derive(Clone, Debug)]
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

/// Raw ranges include their original terminators; handles exclude only terminators.
pub fn parse(input: &[u8], split: Split) -> Result<Vec<Record>, JevifyError> {
    let mut records = Vec::new();
    let mut start = 0;
    let delimiter = if split == Split::Nul { 0 } else { b'\n' };
    let mut block = None;
    for part in input.split_inclusive(|b| [delimiter].contains(b)) {
        let end = start + part.len();
        let mut content = part.strip_suffix(&[delimiter]).unwrap_or(part);
        if split != Split::Nul {
            content = content.strip_suffix(b"\r").unwrap_or(content);
        }
        let blank = content.iter().all(u8::is_ascii_whitespace);
        if split == Split::Para {
            if blank {
                if let Some(begin) = block.take() {
                    records.push(record(
                        input,
                        begin..end,
                        paragraph_handle(input, begin..start),
                    ));
                }
            } else {
                block.get_or_insert(start);
            }
        } else if !blank {
            records.push(record(input, start..end, start..start + content.len()));
        }
        start = end;
    }
    if let Some(begin) = block {
        records.push(record(
            input,
            begin..start,
            paragraph_handle(input, begin..start),
        ));
    }
    Ok(records)
}

fn paragraph_handle(input: &[u8], mut range: Range<usize>) -> Range<usize> {
    if input[range.clone()].ends_with(b"\n") {
        range.end -= 1;
        if input[range.clone()].ends_with(b"\r") {
            range.end -= 1;
        }
    }
    range
}

fn evidence(bytes: &[u8]) -> String {
    let text = crate::input::strip_ansi(&String::from_utf8_lossy(bytes));
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    crate::tournament::clip(&crate::input::redact(&text), 32_000)
}

fn record(input: &[u8], raw: Range<usize>, handle: Range<usize>) -> Record {
    Record {
        handle: OsString::from_vec(input[handle.clone()].to_vec()),
        evidence: evidence(&input[handle]),
        raw,
    }
}

/// Select a 1-based whitespace field, dropping and counting records without it.
pub fn field(records: &mut Vec<Record>, number: usize) -> Result<usize, JevifyError> {
    if number == 0 {
        return Err(JevifyError::Usage("field numbers start at 1".into()));
    }
    let before = records.len();
    records.retain_mut(|record| {
        let value = record
            .handle
            .as_bytes()
            .split(u8::is_ascii_whitespace)
            .filter(|s| !s.is_empty())
            .nth(number - 1);
        if let Some(value) = value {
            record.handle = OsString::from_vec(value.to_vec());
            true
        } else {
            false
        }
    });
    Ok(before - records.len())
}

/// Read JSON lines or one JSON array, preserving each value's source range.
pub fn key(input: &[u8], name: &str) -> Result<(Vec<Record>, usize), JevifyError> {
    let error = |e| JevifyError::Input(format!("invalid JSON: {e}"));
    let mut ranges = Vec::new();
    if matches!(input.iter().find(|b| !b.is_ascii_whitespace()), Some(b'[')) {
        let _: Vec<serde_json::Value> = serde_json::from_slice(input).map_err(error)?;
        let mut pos = input.iter().position(|b| matches!(b, b'[')).unwrap() + 1;
        loop {
            while input[pos].is_ascii_whitespace() || matches!(input[pos], b',') {
                pos += 1;
            }
            if matches!(input[pos], b']') {
                break;
            }
            let mut values = serde_json::Deserializer::from_slice(&input[pos..])
                .into_iter::<serde_json::Value>();
            values
                .next()
                .expect("validated array element")
                .map_err(error)?;
            let end = pos + values.byte_offset();
            ranges.push((pos..end, pos..end));
            pos = end;
        }
    } else {
        for r in parse(input, Split::Lines)? {
            ranges.push((r.raw.clone(), r.raw));
        }
    }
    let mut records = Vec::new();
    let mut omitted = 0;
    for (raw, content) in ranges {
        let value: serde_json::Value =
            serde_json::from_slice(&input[content.clone()]).map_err(error)?;
        if let Some(handle) = value.get(name) {
            records.push(Record {
                handle: handle
                    .as_str()
                    .map_or_else(|| handle.to_string(), str::to_owned)
                    .into(),
                evidence: evidence(&input[content]),
                raw,
            });
        } else {
            omitted += 1;
        }
    }
    Ok((records, omitted))
}

/// `ordinal` is the occurrence's 1-based position before selection or deduplication.
pub fn envelope(input: &[u8], record: &Record, ordinal: usize) -> serde_json::Value {
    let bytes = &input[record.raw.clone()];
    let mut value = serde_json::json!({"text": String::from_utf8_lossy(bytes), "ordinal": ordinal});
    if std::str::from_utf8(bytes).is_err() {
        value["lossy"] = true.into();
    }
    value
}

/// Representative record indices and, for every occurrence, its distinct-set index.
pub fn distinct(input: &[u8], records: &[Record]) -> (Vec<usize>, Vec<usize>) {
    let mut seen = HashMap::new();
    let mut representatives = Vec::new();
    let mut occurrences = Vec::with_capacity(records.len());
    for (index, record) in records.iter().enumerate() {
        let next = representatives.len();
        let distinct = *seen.entry(&input[record.raw.clone()]).or_insert_with(|| {
            representatives.push(index);
            next
        });
        occurrences.push(distinct);
    }
    (representatives, occurrences)
}

/// Judge caller-written components only. `.` and `..` navigation are not hidden components.
pub(crate) fn withheld(path: &Path) -> bool {
    path.components().any(|component| {
        let std::path::Component::Normal(name) = component else {
            return false;
        };
        let bytes = name.as_bytes();
        bytes.starts_with(b".")
            || bytes.starts_with(b"id_")
            || bytes.ends_with(b".pem")
            || bytes.ends_with(b".key")
            || bytes.windows(11).any(|w| matches!(w, b"credentials"))
            || bytes.windows(6).any(|w| matches!(w, b"secret"))
    })
}

/// Enrich only the supplied records (all records or pick's finalists), returning withheld count.
pub async fn excerpts(records: &mut [Record], cwd: &Path) -> Result<usize, JevifyError> {
    let paths: Vec<_> = records.iter().map(|r| r.handle.clone()).collect();
    let cwd = cwd.to_path_buf();
    let (values, count) = tokio::task::spawn_blocking(move || {
        let mut count = 0;
        let values: Vec<_> = paths
            .into_iter()
            .map(|handle| {
                let path = Path::new(&handle);
                let fallback = evidence(handle.as_bytes());
                if withheld(path) {
                    count += 1;
                    return fallback;
                }
                let joined = cwd.join(path);
                let normalized: std::path::PathBuf = joined.components().collect();
                let Some(name) = normalized.file_name() else {
                    return fallback;
                };
                let Some(parent) = normalized.parent().and_then(|p| p.canonicalize().ok()) else {
                    return fallback;
                };
                let resolved = parent.join(name);
                let Ok(metadata) = resolved.symlink_metadata() else {
                    return fallback;
                };
                if metadata.is_symlink() {
                    count += 1;
                    return fallback;
                }
                if !metadata.is_file() {
                    return fallback;
                }
                evidence(crate::cmd::sort::excerpt(&resolved).as_bytes())
            })
            .collect();
        (values, count)
    })
    .await
    .map_err(|e| JevifyError::Input(format!("excerpt worker failed: {e}")))?;
    for (record, value) in records.iter_mut().zip(values) {
        record.evidence = value;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn excerpts_judge_caller_paths_not_working_directory_ancestors() {
        let root = tempfile::Builder::new()
            .prefix(".jevify-records-")
            .tempdir()
            .unwrap()
            .keep();
        eprintln!("retained records fixture: {}", root.display());
        std::fs::write(root.join("source.rs"), "VISIBLE_EXCERPT_MARKER").unwrap();
        let mut records = parse(b"./source.rs\n", Split::Lines).unwrap();
        assert_eq!(excerpts(&mut records, &root).await.unwrap(), 0);
        assert!(records[0].evidence.contains("VISIBLE_EXCERPT_MARKER"));

        let absolute = root.join("source.rs");
        let mut records = parse(absolute.as_os_str().as_bytes(), Split::Nul).unwrap();
        assert_eq!(excerpts(&mut records, &root).await.unwrap(), 1);
        assert_eq!(
            records[0].evidence,
            evidence(absolute.as_os_str().as_bytes())
        );
    }

    #[test]
    fn withheld_uses_lexical_caller_components() {
        for path in ["", ".", "./source.rs", "a/./source.rs", "../source.rs"] {
            assert!(!withheld(Path::new(path)), "{path}");
        }
        for path in [
            ".npmrc",
            "a/.hidden/b.txt",
            "conf/.env.local",
            "/project/.hidden/source.rs",
            "a/.hidden/../source.rs",
        ] {
            assert!(withheld(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn split_boundaries_and_blank_records() {
        for split in [Split::Lines, Split::Nul, Split::Para] {
            assert!(parse(b"", split).unwrap().is_empty());
            let records = parse(b"one", split).unwrap();
            assert_eq!(records[0].raw, 0..3);
            assert_eq!(records[0].handle, "one");
        }
        for (input, split, expected) in [
            (b"a\r\nb\n".as_slice(), Split::Lines, vec![0..3, 3..5]),
            (b"a\0b\0".as_slice(), Split::Nul, vec![0..2, 2..4]),
            (b"a\nb\n\nc\n".as_slice(), Split::Para, vec![0..5, 5..7]),
            (b"a\r\nb\r\n\r\nc".as_slice(), Split::Para, vec![0..8, 8..9]),
            (b"\n \t\r\n".as_slice(), Split::Lines, vec![]),
            (b"\0 \t\0".as_slice(), Split::Nul, vec![]),
            (b"\n \t\r\n".as_slice(), Split::Para, vec![]),
        ] {
            let records = parse(input, split).unwrap();
            assert_eq!(
                records.iter().map(|r| r.raw.clone()).collect::<Vec<_>>(),
                expected
            );
        }
        let input = b"a\nb\n\nc\n";
        let records = parse(input, Split::Para).unwrap();
        assert_eq!(records[0].handle, "a\nb");
        assert_eq!(
            records
                .iter()
                .flat_map(|r| input[r.raw.clone()].iter().copied())
                .collect::<Vec<_>>(),
            input
        );
    }

    #[test]
    fn raw_bytes_and_handles_survive_clean_evidence() {
        let input = b"\x1b[31mred\x1b[0m\0\xff\r\n";
        let records = parse(input, Split::Lines).unwrap();
        assert_eq!(&input[records[0].raw.clone()], input);
        assert_eq!(records[0].handle.as_bytes(), &input[..input.len() - 2]);
        assert_eq!(records[0].evidence, "red\u{fffd}");
        let paths = parse(b"dir/\xff\0", Split::Nul).unwrap();
        assert_eq!(paths[0].handle.as_bytes(), b"dir/\xff");
        let mut fields = parse(b"one \xff\n", Split::Lines).unwrap();
        assert_eq!(field(&mut fields, 2).unwrap(), 0);
        assert_eq!(fields[0].handle.as_bytes(), b"\xff");
        assert_eq!(fields[0].evidence, "one \u{fffd}");
        assert_eq!(evidence(b"token=abcdefghijk"), "token=[REDACTED]");
        assert_eq!(evidence(&vec![b'x'; 32_001]).chars().count(), 32_000);
    }

    #[test]
    fn envelope_and_distinct_preserve_occurrences() {
        let input = b"same\n\xff\nsame\n\xff\n";
        let records = parse(input, Split::Lines).unwrap();
        let (unique, map) = distinct(input, &records);
        assert_eq!(unique, [0, 1]);
        assert_eq!(map, [0, 1, 0, 1]);
        for (i, r) in records.iter().enumerate() {
            assert_eq!(
                &input[r.raw.clone()],
                &input[records[unique[map[i]]].raw.clone()]
            );
            let value = envelope(input, r, i + 1);
            assert_eq!(value["ordinal"], i + 1);
            assert_eq!(
                value["text"],
                String::from_utf8_lossy(&input[r.raw.clone()]).as_ref()
            );
            assert_eq!(
                value.get("lossy"),
                if i % 2 == 1 {
                    Some(&serde_json::Value::Bool(true))
                } else {
                    None
                }
            );
        }
    }

    #[test]
    fn fields_and_json_handles() {
        for (number, expected, omitted) in
            [(1, Some("one"), 0), (3, Some("three"), 0), (4, None, 1)]
        {
            let mut records = parse(b" one\t  two   three\n", Split::Lines).unwrap();
            assert_eq!(field(&mut records, number).unwrap(), omitted);
            assert_eq!(
                records.first().map(|r| r.handle.to_str().unwrap()),
                expected
            );
        }
        assert!(field(&mut vec![], 0).is_err());
        for input in [
            b"{\"id\":\"a\"}\n{\"id\":12}\n{}\n".as_slice(),
            b"[ {\"id\":\"a\"}, {\"id\":12}, {} ]",
        ] {
            let (records, omitted) = key(input, "id").unwrap();
            assert_eq!(omitted, 1);
            assert_eq!(
                records
                    .iter()
                    .map(|r| r.handle.as_bytes())
                    .collect::<Vec<_>>(),
                [b"a".as_slice(), b"12"]
            );
            for record in records {
                let value: serde_json::Value = serde_json::from_slice(&input[record.raw]).unwrap();
                assert!(value.get("id").is_some());
            }
        }
        for input in [
            b"[".as_slice(),
            b"[{},]",
            b"no json",
            b"{} {}",
            b"{\"id\":\"\xff\"}",
        ] {
            assert_eq!(key(input, "id").unwrap_err().exit().code(), 6);
        }
        assert!(key(b"[]", "id").unwrap().0.is_empty());
        let (nested, _) = key(br#"[{"id":{"a":[1,2]}},{"id":"a,b]"}]"#, "id").unwrap();
        assert_eq!(nested[1].handle, "a,b]");
    }

    #[tokio::test]
    async fn excerpts_resolve_paths_and_withhold_before_reading() {
        // Keep fixtures: the worker contract forbids deleting even scratch files.
        let root = tempfile::Builder::new()
            .prefix("jevify-records-")
            .tempdir()
            .unwrap()
            .keep();
        std::fs::create_dir(root.join("a")).unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("a/.hidden")).unwrap();
        let marker = "PRIVATE_EXCERPT_MARKER";
        for path in [
            ".npmrc",
            ".env.local",
            "id_rsa",
            "x.pem",
            "a/.hidden/b.txt",
            "my-credentials.json",
            "x.key",
            "my-secret.txt",
        ] {
            std::fs::write(root.join(path), marker).unwrap();
        }
        for path in ["a.txt", "b.txt", "src/main.rs"] {
            std::fs::write(root.join(path), "VISIBLE_EXCERPT_MARKER").unwrap();
        }
        std::os::unix::fs::symlink(root.join("a.txt"), root.join("link.txt")).unwrap();
        std::os::unix::fs::symlink(&root, root.join("ancestor")).unwrap();
        let input = b"./a.txt\0a/../b.txt\0ancestor/a.txt\0src/main.rs\0missing\0a\0.npmrc\0.env.local\0id_rsa\0x.pem\0a/.hidden/b.txt\0my-credentials.json\0link.txt\0x.key\0my-secret.txt\0";
        let mut records = parse(input, Split::Nul).unwrap();
        assert_eq!(excerpts(&mut records, &root).await.unwrap(), 9);
        for r in &records[..4] {
            assert!(
                r.evidence.contains("VISIBLE_EXCERPT_MARKER"),
                "{}",
                r.evidence
            );
        }
        for r in &records[4..] {
            assert_eq!(r.evidence, evidence(r.handle.as_bytes()));
        }
        assert!(records.iter().all(|r| !r.evidence.contains(marker)));
        assert_eq!(records.len(), 15);
        let mut subset = parse(b"a.txt\nmissing\n", Split::Lines).unwrap();
        excerpts(&mut subset[..1], &root).await.unwrap();
        assert_eq!(subset[1].evidence, "missing");
        eprintln!("retained records fixture: {}", root.display());
    }

    #[tokio::test]
    async fn non_utf8_file_excerpt() {
        let root = tempfile::Builder::new()
            .prefix("jevify-records-")
            .tempdir()
            .unwrap()
            .keep();
        eprintln!("retained records fixture: {}", root.display());
        let name = OsString::from_vec(b"file-\xff".to_vec());
        let file_created = match std::fs::write(root.join(&name), "VISIBLE_EXCERPT_MARKER") {
            Ok(()) => true,
            Err(error) => {
                // EILSEQ differs between the two supported platforms.
                assert!(
                    matches!(
                        (std::env::consts::OS, error.raw_os_error()),
                        ("macos", Some(92)) | ("linux", Some(84))
                    ),
                    "non-UTF-8 fixture creation failed: {error}"
                );
                eprintln!(
                    "NOT RUN: non-UTF-8 file-system excerpt assertion: file system refused the name with EILSEQ: {error}"
                );
                false
            }
        };
        let mut records = parse(b"file-\xff\0", Split::Nul).unwrap();
        assert_eq!(excerpts(&mut records, &root).await.unwrap(), 0);
        assert_eq!(records[0].handle.as_bytes(), b"file-\xff");
        if file_created {
            assert!(records[0].evidence.contains("VISIBLE_EXCERPT_MARKER"));
        } else {
            assert_eq!(records[0].evidence, "file-\u{fffd}");
        }
    }
}
