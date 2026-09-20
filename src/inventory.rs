use crate::exit::JevifyError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Tool {
    pub name: String,
    pub summary: String,
}

static ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([^\s,()]+)\s*\(([0-9][A-Za-z0-9]*)\)").unwrap());

pub fn parse_whatis(text: &str, executables: &HashSet<String>) -> Vec<Tool> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for line in text.lines() {
        let Some((names, desc)) = line.split_once(" - ") else {
            continue;
        };
        let desc = desc.trim();
        for cap in ENTRY.captures_iter(names) {
            let (name, section) = (&cap[1], &cap[2]);
            if matches!(section.chars().next(), Some('1' | '6' | '8')) && executables.contains(name)
            {
                out.entry(name.to_string())
                    .or_insert_with(|| desc.to_string());
            }
        }
    }
    out.into_iter()
        .map(|(name, summary)| Tool { name, summary })
        .collect()
}

fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default()
}

fn executables(dirs: &[PathBuf]) -> HashSet<String> {
    let mut set = HashSet::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else {
            continue;
        };
        for e in rd.flatten() {
            // DirEntry::metadata does not follow symlinks; Homebrew and alternatives tools are symlinks.
            if let Ok(md) = std::fs::metadata(e.path()) {
                if md.is_file() && md.permissions().mode() & 0o111 != 0 {
                    set.insert(e.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    set
}

fn whatis_text() -> String {
    let manpath = Command::new("manpath")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let mut text = String::new();
    for dir in manpath.split(':').filter(|d| !d.is_empty()) {
        if let Ok(t) = std::fs::read_to_string(Path::new(dir).join("whatis")) {
            text.push_str(&t);
            text.push('\n');
        }
    }
    if text.trim().is_empty() {
        // No plain whatis files (man-db on Linux, and recent macOS): ask man for its index.
        // Cold, this can regenerate the database (~2 s); the inventory cache makes it a one-off.
        if let Ok(o) = Command::new("man")
            .args(["-k", "."])
            .env("MANPAGER", "cat")
            .output()
        {
            text = String::from_utf8_lossy(&o.stdout).into_owned();
        }
    }
    text
}

fn fingerprint(dirs: &[PathBuf]) -> String {
    let mut h = blake3::Hasher::new();
    for d in dirs {
        h.update(d.as_os_str().as_encoded_bytes());
        if let Ok(m) = std::fs::metadata(d).and_then(|m| m.modified()) {
            h.update(format!("{m:?}").as_bytes());
        }
    }
    h.finalize().to_hex()[..16].to_string()
}

pub fn load(cache_dir: Option<&Path>) -> Result<Vec<Tool>, JevifyError> {
    let dirs = path_dirs();
    let cache_file = cache_dir.map(|c| c.join(format!("inventory-{}.json", fingerprint(&dirs))));
    if let Some(tools) = cache_file
        .as_ref()
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| serde_json::from_slice::<Vec<Tool>>(&b).ok())
    {
        return Ok(tools);
    }
    let exes = executables(&dirs);
    let mut tools = parse_whatis(&whatis_text(), &exes);
    // Tools people actually reach for (rg, fd, uv, ...) often ship no man page, and minimal
    // Linux images ship no whatis database at all: list those by name rather than hide them.
    let documented: HashSet<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    let system = |d: &Path| {
        ["/bin", "/sbin", "/usr/bin", "/usr/sbin", "/usr/libexec"]
            .iter()
            .any(|s| d == Path::new(s))
    };
    // No man index at all (containers, or `man -k` refused inside a sandbox): names only, and
    // that degraded list is never cached, so the next run with a working `man` rebuilds it.
    let names_only = tools.is_empty();
    // 1,000 names cost ~5k tokens and a few windows; 400 dropped uv and yq on a Mac with ~600
    // undocumented tools on PATH.
    let (extra_dirs, cap): (Vec<PathBuf>, usize) = if names_only {
        (dirs.clone(), 1_500)
    } else {
        (dirs.iter().filter(|d| !system(d)).cloned().collect(), 1_000)
    };
    // Cap in PATH order (alphabetical within a directory), not alphabetically overall: the
    // user's own directories come first on PATH, and an alphabetical cut would drop late-alphabet
    // tools such as uv, yq or zoxide on a Homebrew-heavy machine.
    let mut extra: Vec<String> = Vec::new();
    for d in &extra_dirs {
        let mut names: Vec<String> = executables(std::slice::from_ref(d))
            .into_iter()
            .filter(|n| !documented.contains(n.as_str()) && !extra.contains(n))
            .collect();
        names.sort();
        extra.extend(names);
        if extra.len() >= cap {
            break;
        }
    }
    extra.truncate(cap);
    tools.extend(extra.into_iter().map(|name| Tool {
        name,
        summary: "(no man page)".into(),
    }));
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    if tools.is_empty() {
        return Err(JevifyError::Input("no executables found on PATH".into()));
    }
    if let Some(f) = cache_file.filter(|_| !names_only) {
        if let Some(p) = f.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let _ = std::fs::write(&f, serde_json::to_vec(&tools).unwrap_or_default());
    }
    Ok(tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_macos_and_linux_whatis_lines() {
        let exe: std::collections::HashSet<String> = ["tar", "bsdtar", "curl", "drutil"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let text = "tar(1), bsdtar(1)        - manipulate tape archives\ncurl (1)             - transfer a URL\nfopen(3)                 - stream open functions\ndrutil(1)                - interact with CD/DVD burners\n";
        let tools = parse_whatis(text, &exe);
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["bsdtar", "curl", "drutil", "tar"]);
        assert_eq!(tools[3].summary, "manipulate tape archives");
    }
    #[test]
    fn executables_follow_symlinks() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        std::fs::write(&real, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&real, d.path().join("link")).unwrap();
        let set = executables(&[d.path().to_path_buf()]);
        assert!(set.contains("real") && set.contains("link"));
    }
}
