use crate::{
    exit::JevifyError,
    records::{Record, Split},
};
use std::{
    borrow::Cow,
    collections::{BTreeSet, HashSet},
    ffi::{OsStr, OsString},
    io::Read,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, LazyLock,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// The coded kinds, then the shipped recipes of `src/kinds.jsonl`, in file order.
pub const KINDS: &[&str] = &[
    "-",
    "branch",
    "pr",
    "issue",
    "ci-run",
    "stash",
    "process",
    "container",
    "pod",
];
pub const LISTER_TIMEOUT: Duration = Duration::from_secs(20);
const OUTPUT_CAP: usize = 64 * 1024 * 1024;
const READER_GRACE: Duration = Duration::from_millis(200);
/// The shipped recipes, one JSON object per line.
const SHIPPED: &str = include_str!("kinds.jsonl");
/// The user's recipes, in the configuration directory only, never in the working directory.
const USER_FILE: &str = "kinds.jsonl";
const BRANCH_ARGV: [&str; 6] = [
    "git",
    "for-each-ref",
    "--sort=-committerdate",
    "--format=%(refname)%00%(symref)%00%(committerdate:unix)%00%(subject)%00",
    "refs/heads",
    "refs/remotes",
];

#[derive(Debug, Clone)]
pub struct Kind {
    pub name: Cow<'static, str>,
    pub coded: bool,
    pub ordered: bool,
    pub path_kind: bool,
    pub has_tier_two: bool,
}

const fn recipe_kind(name: &'static str, ordered: bool) -> Kind {
    Kind {
        name: Cow::Borrowed(name),
        coded: false,
        ordered,
        path_kind: false,
        has_tier_two: false,
    }
}

pub const REGISTRY: &[Kind] = &[
    Kind {
        name: Cow::Borrowed("-"),
        coded: true,
        ordered: false,
        path_kind: false,
        has_tier_two: false,
    },
    Kind {
        name: Cow::Borrowed("branch"),
        coded: true,
        ordered: true,
        path_kind: false,
        has_tier_two: true,
    },
    recipe_kind("pr", true),
    recipe_kind("issue", true),
    recipe_kind("ci-run", true),
    recipe_kind("stash", true),
    recipe_kind("process", false),
    recipe_kind("container", false),
    recipe_kind("pod", false),
];

/// A shipped kind: coded, or a recipe of `src/kinds.jsonl`. User recipes need [`lookup`].
pub fn kind(name: &str) -> Option<&'static Kind> {
    REGISTRY.iter().find(|kind| kind.name == name)
}

/// A kind as one JSON line: the lister argv and the handle.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub kind: String,
    pub list: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default)]
    pub ordered: bool,
}

fn valid_kind_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b == b'-')
}

fn parse_recipe(line: &[u8]) -> Result<Recipe, String> {
    let recipe: Recipe = serde_json::from_slice(line).map_err(|e| e.to_string())?;
    if !valid_kind_name(&recipe.kind) {
        return Err(format!(
            "kind {:?} does not match [a-z][a-z-]*",
            recipe.kind
        ));
    }
    if recipe.list.is_empty() {
        return Err(format!("kind {}: list is empty", recipe.kind));
    }
    if recipe.field.is_some() && recipe.key.is_some() {
        return Err(format!(
            "kind {}: field and key are mutually exclusive",
            recipe.kind
        ));
    }
    if recipe.field == Some(0) {
        return Err(format!("kind {}: field numbers start at 1", recipe.kind));
    }
    Ok(recipe)
}

fn shipped() -> &'static [Recipe] {
    static SHIPPED_RECIPES: LazyLock<Vec<Recipe>> = LazyLock::new(|| {
        SHIPPED
            .lines()
            .map(|line| parse_recipe(line.as_bytes()).expect("a shipped recipe parses"))
            .collect()
    });
    &SHIPPED_RECIPES
}

/// Read the user's `kinds.jsonl` from `env.config_dir`. Every line must parse and no line may
/// name a shipped kind, or the whole file is `recipe_invalid`. A missing file holds no recipe.
fn user_recipes(env: &Env) -> Result<Vec<Recipe>, JevifyError> {
    let Some(dir) = &env.config_dir else {
        return Ok(Vec::new());
    };
    let path = dir.join(USER_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(JevifyError::recipe_invalid(format!(
                "{}: {e}",
                path.display()
            )));
        }
    };
    let mut recipes: Vec<Recipe> = Vec::new();
    for (index, line) in bytes.split(|b| *b == b'\n').enumerate() {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let invalid = |message: String| {
            JevifyError::recipe_invalid(format!("{} line {}: {message}", path.display(), index + 1))
        };
        let recipe = parse_recipe(line).map_err(invalid)?;
        if kind(&recipe.kind).is_some() {
            return Err(invalid(format!(
                "kind {} is built in and cannot be replaced",
                recipe.kind
            )));
        }
        if recipes.iter().any(|r| r.kind == recipe.kind) {
            return Err(invalid(format!("kind {} is defined twice", recipe.kind)));
        }
        recipes.push(recipe);
    }
    Ok(recipes)
}

/// Find a kind: coded kinds and shipped recipes first, and only for a name in neither, the
/// user's `kinds.jsonl`. `Ok(None)` is a name found nowhere.
pub fn lookup(name: &str, env: &Env) -> Result<Option<Kind>, JevifyError> {
    if let Some(kind) = kind(name) {
        return Ok(Some(kind.clone()));
    }
    Ok(user_recipes(env)?
        .into_iter()
        .find(|recipe| recipe.kind == name)
        .map(|recipe| Kind {
            name: Cow::Owned(recipe.kind),
            coded: false,
            ordered: recipe.ordered,
            path_kind: false,
            has_tier_two: false,
        }))
}

/// The recipe of a kind that is not coded, with the same read rules as [`lookup`].
fn recipe(name: &str, env: &Env) -> Result<Option<Recipe>, JevifyError> {
    if kind(name).is_some_and(|kind| kind.coded) {
        return Ok(None);
    }
    if let Some(recipe) = shipped().iter().find(|recipe| recipe.kind == name) {
        return Ok(Some(recipe.clone()));
    }
    Ok(user_recipes(env)?
        .into_iter()
        .find(|recipe| recipe.kind == name))
}

/// One kind as `capabilities` prints it: where it comes from and the argv of its lister.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CatalogEntry {
    pub name: String,
    /// `coded`, `shipped` or `user`.
    pub origin: &'static str,
    /// Empty for `-`, which reads stdin or `--candidates`.
    pub list: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Catalog {
    pub kinds: Vec<CatalogEntry>,
    /// Why the user's `kinds.jsonl` was not listed; a bad file never fails the caller.
    pub error: Option<String>,
}

/// Every kind with its lister argv, the user's recipes included.
pub fn catalog(env: &Env) -> Catalog {
    let mut kinds = vec![
        CatalogEntry {
            name: "-".into(),
            origin: "coded",
            list: Vec::new(),
        },
        CatalogEntry {
            name: "branch".into(),
            origin: "coded",
            list: BRANCH_ARGV.iter().map(|arg| (*arg).to_owned()).collect(),
        },
    ];
    let entry = |recipe: &Recipe, origin| CatalogEntry {
        name: recipe.kind.clone(),
        origin,
        list: recipe.list.clone(),
    };
    kinds.extend(shipped().iter().map(|recipe| entry(recipe, "shipped")));
    let error = match user_recipes(env) {
        Ok(recipes) => {
            kinds.extend(recipes.iter().map(|recipe| entry(recipe, "user")));
            None
        }
        Err(e) => Some(e.to_string()),
    };
    Catalog { kinds, error }
}

pub enum Scope {
    Input {
        bytes: Vec<u8>,
        split: Split,
        field: Option<usize>,
        key: Option<String>,
    },
    Prefix(Option<PathBuf>),
}

#[derive(Debug)]
pub struct Listing {
    pub records: Vec<Record>,
    pub total: usize,
    pub omitted: usize,
    pub ordered: bool,
}

#[derive(Clone)]
pub struct Env {
    pub path: OsString,
    pub config_dir: Option<PathBuf>,
    pub deadline: Instant,
    pub cwd: PathBuf,
}

impl Env {
    pub fn from_process(timeout: Duration) -> Self {
        let vars: std::collections::HashMap<_, _> = std::env::vars_os().collect();
        Self {
            path: vars
                .get(std::ffi::OsStr::new("PATH"))
                .cloned()
                .unwrap_or_default(),
            config_dir: crate::config::config_dir(
                vars.get(std::ffi::OsStr::new("JEVIFY_CONFIG_DIR"))
                    .and_then(|value| value.to_str()),
            ),
            deadline: Instant::now() + timeout,
            // An unavailable cwd must fail in Command, never silently list another directory.
            cwd: std::env::current_dir().unwrap_or_default(),
        }
    }
}

pub async fn enumerate(
    kind: &str,
    scope: Scope,
    limit: usize,
    env: &Env,
) -> Result<Listing, JevifyError> {
    let kind = kind.to_owned();
    let env = env.clone();
    tokio::task::spawn_blocking(move || match (kind.as_str(), scope) {
        (
            "-",
            Scope::Input {
                bytes,
                split,
                field,
                key,
            },
        ) => input_listing(&bytes, split, field, key.as_deref(), false, usize::MAX),
        ("branch", Scope::Prefix(None)) => branches(limit, &env),
        (name, Scope::Prefix(None)) => match recipe(name, &env)? {
            Some(recipe) => recipe_listing(&recipe, limit, &env),
            None => Err(JevifyError::Usage(format!("unknown kind {name}"))),
        },
        _ => Err(JevifyError::Usage(format!(
            "invalid kind or scope for {kind}"
        ))),
    })
    .await
    .map_err(|e| JevifyError::lister_failed(e.to_string()))?
}

pub async fn enrich(kind: &str, handles: &[OsString]) -> Vec<String> {
    enrich_with_env(kind, handles, &Env::from_process(LISTER_TIMEOUT)).await
}

/// Missing enrichment is empty evidence; no partial lister output is returned.
pub async fn enrich_with_env(kind: &str, handles: &[OsString], env: &Env) -> Vec<String> {
    if kind != "branch" {
        return vec![String::new(); handles.len()];
    }
    let env = env.clone();
    let handles = handles.to_vec();
    let count = handles.len();
    tokio::task::spawn_blocking(move || {
        handles
            .iter()
            .map(|handle| branch_evidence(handle, &env).unwrap_or_default())
            .collect()
    })
    .await
    .unwrap_or_else(|_| vec![String::new(); count])
}

fn input_listing(
    bytes: &[u8],
    split: Split,
    field: Option<usize>,
    key: Option<&str>,
    ordered: bool,
    limit: usize,
) -> Result<Listing, JevifyError> {
    if field.is_some() && key.is_some() {
        return Err(JevifyError::Usage(
            "field and key are mutually exclusive".into(),
        ));
    }
    let (mut records, mut omitted) = if let Some(key) = key {
        crate::records::key(bytes, key)?
    } else {
        (crate::records::parse(bytes, split)?, 0)
    };
    if let Some(field) = field {
        omitted += crate::records::field(&mut records, field)?;
    }
    Ok(listing(records, omitted, ordered, limit))
}

/// The `-` path over a lister's output. The evidence is the whole record; the limit only cuts
/// an ordered listing, and the argv is never rewritten.
fn recipe_listing(recipe: &Recipe, limit: usize, env: &Env) -> Result<Listing, JevifyError> {
    let argv: Vec<OsString> = recipe.list.iter().map(OsString::from).collect();
    let bytes = run_lister_blocking(&argv, env, OUTPUT_CAP)?;
    input_listing(
        &bytes,
        Split::Lines,
        recipe.field,
        recipe.key.as_deref(),
        recipe.ordered,
        limit,
    )
    .map_err(|e| JevifyError::lister_failed(format!("{}: {e}", recipe.list[0])))
}

fn listing(mut records: Vec<Record>, mut omitted: usize, ordered: bool, limit: usize) -> Listing {
    let mut seen = HashSet::new();
    records.retain(|record| {
        if record
            .handle
            .as_bytes()
            .iter()
            .any(|b| matches!(b, b'\n' | b'\r' | 0))
        {
            omitted += 1;
            return false;
        }
        seen.insert((record.handle.clone(), record.evidence.clone()))
    });
    let total = records.len();
    if ordered {
        records.truncate(limit);
    }
    Listing {
        records,
        total,
        omitted,
        ordered,
    }
}

/// Run an argv on the blocking pool. Success requires exit 0 and EOF on both pipes.
pub async fn run_lister(argv: &[OsString], env: &Env) -> Result<Vec<u8>, JevifyError> {
    let argv = argv.to_vec();
    let env = env.clone();
    tokio::task::spawn_blocking(move || run_lister_blocking(&argv, &env, OUTPUT_CAP))
        .await
        .map_err(|e| JevifyError::lister_failed(e.to_string()))?
}

fn read_pipe(mut pipe: impl Read, cap: usize, size: &AtomicUsize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let n = match pipe.read(&mut buffer) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.to_string()),
        };
        if n == 0 {
            return Ok(bytes);
        }
        if size.fetch_add(n, Ordering::Relaxed).saturating_add(n) > cap {
            return Err("output exceeds lister byte cap".into());
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
}

fn run_lister_blocking(argv: &[OsString], env: &Env, cap: usize) -> Result<Vec<u8>, JevifyError> {
    let Some(program) = argv.first() else {
        return Err(JevifyError::lister_failed("empty lister argv".into()));
    };
    let fail = |message: String| {
        JevifyError::lister_failed(format!("{}: {message}", program.to_string_lossy()))
    };
    if Instant::now() >= env.deadline {
        return Err(fail("deadline exceeded".into()));
    }
    let mut command = Command::new(program);
    command
        .args(&argv[1..])
        .current_dir(&env.cwd)
        .env("PATH", &env.path)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = &env.config_dir {
        command.env("JEVIFY_CONFIG_DIR", dir);
    } else {
        command.env_remove("JEVIFY_CONFIG_DIR");
    }
    let mut child = command.spawn().map_err(|e| fail(e.to_string()))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, rx) = mpsc::channel();
    let size = Arc::new(AtomicUsize::new(0));
    let out_size = Arc::clone(&size);
    let out_tx = tx.clone();
    std::thread::spawn(move || {
        let _ = out_tx.send((0, read_pipe(stdout, cap, &out_size)));
    });
    std::thread::spawn(move || {
        let _ = tx.send((1, read_pipe(stderr, cap, &size)));
    });
    let mut streams = [None, None];
    let mut failure = None;
    let status = loop {
        while let Ok((index, result)) = rx.try_recv() {
            streams[index] = Some(result);
        }
        if let Some(error) = streams.iter().flatten().find_map(|s| s.as_ref().err()) {
            failure = Some(error.clone());
        }
        if Instant::now() >= env.deadline {
            failure = Some("deadline exceeded".into());
        }
        if failure.is_some() {
            let _ = child.kill();
            break child.wait().map_err(|e| fail(e.to_string()))?;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(
                Duration::from_millis(20)
                    .min(env.deadline.saturating_duration_since(Instant::now())),
            ),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                failure = Some(e.to_string());
                break std::process::ExitStatus::default();
            }
        }
    };
    let grace = Instant::now() + READER_GRACE;
    while streams.iter().any(Option::is_none) {
        match rx.recv_timeout(grace.saturating_duration_since(Instant::now())) {
            Ok((index, result)) => streams[index] = Some(result),
            Err(_) => {
                failure.get_or_insert("lister pipes did not reach EOF".into());
                break;
            }
        }
    }
    for error in streams.iter().flatten().filter_map(|s| s.as_ref().err()) {
        failure.get_or_insert(error.clone());
    }
    if !status.success() {
        failure.get_or_insert(format!("exited with {status}"));
    }
    if let Some(mut message) = failure {
        if let Some(Ok(stderr)) = &streams[1] {
            let tail = &stderr[stderr.len().saturating_sub(4096)..];
            if !tail.is_empty() {
                message.push_str(&format!(": {}", String::from_utf8_lossy(tail)));
            }
        }
        return Err(fail(message));
    }
    streams[0]
        .take()
        .expect("both readers finished")
        .map_err(fail)
}

fn branches(limit: usize, env: &Env) -> Result<Listing, JevifyError> {
    let bytes = run_lister_blocking(&BRANCH_ARGV.map(OsString::from), env, OUTPUT_CAP)?;
    let mut refs = Vec::new();
    for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
        let parts: Vec<_> = line.split(|b| *b == 0).collect();
        if parts.len() != 5 || !parts[4].is_empty() {
            return Err(JevifyError::lister_failed(
                "malformed git ref listing".into(),
            ));
        }
        if !parts[1].is_empty() {
            continue;
        }
        let timestamp = std::str::from_utf8(parts[2])
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| JevifyError::lister_failed("invalid git committer date".into()))?;
        refs.push((parts[0], parts[3], timestamp));
    }
    let locals: HashSet<_> = refs
        .iter()
        .filter_map(|(name, _, _)| name.strip_prefix(b"refs/heads/"))
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut records = Vec::new();
    for (name, subject, timestamp) in &refs {
        let handle = if let Some(local) = name.strip_prefix(b"refs/heads/") {
            local
        } else if let Some(remote) = name.strip_prefix(b"refs/remotes/") {
            if let Some(slash) = remote.iter().position(|b| *b == b'/') {
                if locals.contains(&remote[slash + 1..]) {
                    continue;
                }
            }
            remote
        } else {
            return Err(JevifyError::lister_failed(
                "unexpected git ref namespace".into(),
            ));
        };
        records.push(Record {
            handle: OsString::from_vec(handle.to_vec()),
            evidence: format!(
                "{} — {} — {}",
                String::from_utf8_lossy(handle),
                String::from_utf8_lossy(subject),
                age(now, *timestamp)
            ),
            raw: 0..0,
        });
    }
    Ok(listing(records, 0, true, limit))
}

fn age(now: u64, timestamp: u64) -> String {
    let seconds = now.saturating_sub(timestamp);
    let (count, unit) = if seconds >= 86400 {
        (seconds / 86400, "day")
    } else if seconds >= 3600 {
        (seconds / 3600, "hour")
    } else if seconds >= 60 {
        (seconds / 60, "minute")
    } else {
        (seconds, "second")
    };
    format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" })
}

fn branch_evidence(handle: &OsStr, env: &Env) -> Result<String, JevifyError> {
    let bytes = run_lister_blocking(
        &[
            "git".into(),
            "log".into(),
            "-5".into(),
            "--format=%x00%s%x00".into(),
            "--name-only".into(),
            "-z".into(),
            "--no-renames".into(),
            "--no-ext-diff".into(),
            "--end-of-options".into(),
            handle.to_owned(),
            "--".into(),
        ],
        env,
        OUTPUT_CAP,
    )?;
    // Each commit starts with an empty NUL field, then its subject. Paths follow.
    let fields: Vec<_> = bytes.split(|b| *b == 0).collect();
    let mut subjects = Vec::new();
    let mut paths = BTreeSet::new();
    let mut index = 0;
    while index + 1 < fields.len() {
        if fields[index].is_empty() {
            index += 1;
            subjects.push(String::from_utf8_lossy(fields[index]).into_owned());
            index += 1;
            // -z adds a NUL after the formatted subject's own NUL.
            if index < fields.len() && fields[index].is_empty() {
                index += 1;
            }
        } else {
            let path = fields[index].strip_prefix(b"\n").unwrap_or(fields[index]);
            if !path.is_empty() {
                paths.insert(
                    String::from_utf8_lossy(path.split(|b| *b == b'/').next().unwrap_or(path))
                        .into_owned(),
                );
            }
            index += 1;
        }
    }
    Ok(format!(
        "{}\nLast 5 subjects:\n{}\nChanged top-level paths: {}",
        handle.to_string_lossy(),
        subjects.join("\n"),
        paths.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    fn scratch() -> PathBuf {
        tempfile::Builder::new()
            .prefix("jevify-source-")
            .tempdir()
            .unwrap()
            .keep()
            .canonicalize()
            .unwrap()
    }

    fn environment(cwd: &Path) -> Env {
        Env {
            path: "/usr/bin:/bin".into(),
            cwd: cwd.into(),
            config_dir: None,
            deadline: Instant::now() + LISTER_TIMEOUT,
        }
    }

    fn fake_git(body: &str) -> Env {
        let dir = scratch();
        let git = dir.join("git");
        fs::write(&git, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
        Env {
            path: dir.as_os_str().to_owned(),
            ..environment(&dir)
        }
    }

    fn input(bytes: &[u8], split: Split, field: Option<usize>, key: Option<&str>) -> Scope {
        Scope::Input {
            bytes: bytes.to_vec(),
            split,
            field,
            key: key.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn input_records_preserve_bytes_and_ranges_and_ignore_unordered_limit() {
        let bytes = b"first\nfirst\n-option\n\xff\n";
        let env = environment(Path::new("."));
        let result = enumerate("-", input(bytes, Split::Lines, None, None), 1, &env)
            .await
            .unwrap();
        assert_eq!(result.total, 3);
        assert_eq!(result.omitted, 0);
        assert!(!result.ordered);
        assert_eq!(result.records.len(), 3);
        assert_eq!(result.records[0].handle, "first");
        assert_eq!(&bytes[result.records[0].raw.clone()], b"first\n");
        assert_eq!(result.records[1].handle, "-option");
        assert_eq!(result.records[2].handle.as_bytes(), b"\xff");
        assert!(
            enumerate("-", input(b"", Split::Lines, None, None), 1, &env)
                .await
                .unwrap()
                .records
                .is_empty()
        );
        let result = enumerate(
            "-",
            input(b"a\0bad\nname\0bad\rname\0", Split::Nul, None, None),
            10,
            &env,
        )
        .await
        .unwrap();
        assert_eq!((result.total, result.omitted), (1, 2));
        let result = enumerate(
            "-",
            input(b"bad\0name\ngood\n", Split::Lines, None, None),
            10,
            &env,
        )
        .await
        .unwrap();
        assert_eq!((result.total, result.omitted), (1, 1));
    }

    #[tokio::test]
    async fn field_and_json_handles_keep_whole_record_evidence() {
        let env = environment(Path::new("."));
        let bytes = b"1 first choice\n2 second choice\nshort\n";
        let result = enumerate("-", input(bytes, Split::Lines, Some(2), None), 10, &env)
            .await
            .unwrap();
        assert_eq!(result.records[0].handle, "first");
        assert_eq!(result.records[0].evidence, "1 first choice");
        assert_eq!((result.total, result.omitted), (2, 1));
        for bytes in [
            b"{\"id\":1,\"title\":\"first\"}\n{\"id\":2,\"title\":\"second\"}\n".as_slice(),
            b"[{\"id\":1,\"title\":\"first\"},{\"id\":2,\"title\":\"second\"}]".as_slice(),
        ] {
            let result = enumerate("-", input(bytes, Split::Lines, None, Some("id")), 10, &env)
                .await
                .unwrap();
            assert_eq!(result.records[0].handle, "1");
            assert_eq!(result.records[1].handle, "2");
            assert!(result.records[0].evidence.contains("first"));
            assert!(bytes[result.records[0].raw.clone()].starts_with(b"{"));
        }
        let result = enumerate("-", input(br#"[{"id":"bad\nname"},{"id":"bad\rname"},{"id":"bad\u0000name"},{"id":"-ok"},{}]"#, Split::Lines, None, Some("id")), 10, &env).await.unwrap();
        assert_eq!((result.total, result.omitted), (1, 4));
        assert_eq!(result.records[0].handle, "-ok");
        for scope in [
            input(b"x", Split::Lines, Some(0), None),
            input(b"x", Split::Lines, Some(1), Some("id")),
            Scope::Prefix(None),
        ] {
            assert_eq!(
                enumerate("-", scope, 10, &env).await.unwrap_err().kind(),
                "usage"
            );
        }
        assert_eq!(
            enumerate(
                "-",
                input(b"invalid json", Split::Lines, None, Some("id")),
                10,
                &env
            )
            .await
            .unwrap_err()
            .kind(),
            "input"
        );
        assert_eq!(
            enumerate("branch", Scope::Prefix(Some("src".into())), 10, &env)
                .await
                .unwrap_err()
                .kind(),
            "usage"
        );
        assert_eq!(
            enumerate("unknown", Scope::Prefix(None), 10, &env)
                .await
                .unwrap_err()
                .kind(),
            "usage"
        );
        assert_eq!(
            enrich_with_env("-", &["x".into(), "y".into()], &env).await,
            ["", ""]
        );
    }

    #[test]
    fn deduplication_uses_handle_and_evidence_and_age_is_computed() {
        let record = |handle: &str, evidence: &str| Record {
            handle: handle.into(),
            evidence: evidence.into(),
            raw: 0..0,
        };
        let result = listing(
            vec![
                record("a", "same"),
                record("b", "same"),
                record("a", "same"),
                record("a", "different"),
            ],
            0,
            false,
            1,
        );
        assert_eq!(result.total, 3);
        assert_eq!(age(172800, 0), "2 days ago");
        assert_eq!(age(3600, 0), "1 hour ago");
        assert_eq!(age(60, 0), "1 minute ago");
        assert_eq!(age(0, 10), "0 seconds ago");
        assert_eq!(REGISTRY.iter().map(|k| &k.name).collect::<Vec<_>>(), KINDS);
        assert!(kind("branch").unwrap().ordered);
        assert!(kind("branch").unwrap().has_tier_two);
        assert!(kind("-").unwrap().coded);
        assert!(!kind("-").unwrap().path_kind);
        assert!(kind("unknown").is_none());
    }

    fn git(dir: &Path, args: &[&str], timestamp: u64) -> Vec<u8> {
        let output = Command::new("/usr/bin/git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .env("GIT_AUTHOR_DATE", format!("@{timestamp} +0000"))
            .env("GIT_COMMITTER_DATE", format!("@{timestamp} +0000"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    #[tokio::test]
    async fn real_branches_fold_twins_order_limit_and_enrich_last_five() {
        let dir = scratch();
        git(&dir, &["init", "--initial-branch=main"], 1700000000);
        let mut commits = Vec::new();
        for i in 0..6 {
            let top = if i == 0 { "sixth-only" } else { "recent" };
            fs::create_dir_all(dir.join(top)).unwrap();
            let file = format!("{top}/file-{i}");
            fs::write(dir.join(&file), format!("{i}\n")).unwrap();
            git(&dir, &["add", "--", &file], 1700000000);
            git(
                &dir,
                &[
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-m",
                    &format!("subject-{i}"),
                ],
                1700000000 + i * 86400,
            );
            commits.push(
                String::from_utf8(git(&dir, &["rev-parse", "HEAD"], 1700000000))
                    .unwrap()
                    .trim()
                    .to_owned(),
            );
        }
        for (name, commit) in [
            ("refs/heads/older", &commits[1]),
            ("refs/heads/middle", &commits[3]),
            ("refs/remotes/origin/remote-only", &commits[4]),
            ("refs/remotes/upstream/ancient", &commits[0]),
            ("refs/remotes/origin/main", &commits[5]),
        ] {
            git(&dir, &["update-ref", name, commit], 1700000000);
        }
        git(
            &dir,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
            1700000000,
        );
        let env = environment(&dir);
        let result = enumerate("branch", Scope::Prefix(None), 3, &env)
            .await
            .unwrap();
        assert_eq!((result.total, result.omitted, result.ordered), (5, 0, true));
        assert_eq!(
            result
                .records
                .iter()
                .map(|r| r.handle.as_os_str())
                .collect::<Vec<_>>(),
            [
                OsStr::new("main"),
                OsStr::new("origin/remote-only"),
                OsStr::new("middle")
            ]
        );
        for record in &result.records {
            assert_eq!(record.raw, 0..0);
        }
        assert!(result.records[0].evidence.contains("subject-5"));
        assert!(result.records[0].evidence.contains("days ago"));
        assert!(
            enumerate("branch", Scope::Prefix(None), 0, &env)
                .await
                .unwrap()
                .records
                .is_empty()
        );
        let evidence = enrich_with_env("branch", &["main".into()], &env).await;
        for i in 1..6 {
            assert!(
                evidence[0].contains(&format!("subject-{i}")),
                "{}",
                evidence[0]
            );
        }
        assert!(!evidence[0].contains("subject-0"));
        assert!(evidence[0].contains("Changed top-level paths: recent"));
        assert!(!evidence[0].contains("sixth-only"));
        let outside = environment(&scratch());
        let error = enumerate("branch", Scope::Prefix(None), 3, &outside)
            .await
            .unwrap_err();
        assert_eq!(error.kind(), "lister_failed");
        assert!(error.to_string().contains("not a git repository"));
    }

    #[tokio::test]
    async fn only_finalists_get_tier_two_and_keep_their_order() {
        let env = fake_git(
            r#"
printf '%s\n' "$*" >> calls
if [ "$1" = for-each-ref ]; then
    i=50
    while [ "$i" -gt 0 ]; do
        printf 'refs/heads/b%s\000\000%s\000tip-%s\000\n' "$i" "$i" "$i"
        i=$((i - 1))
    done
else
    for arg in "$@"; do
        case "$arg" in b*) printf '\000subject-%s\000\000\npath/file\000' "$arg";; esac
    done
fi
"#,
        );
        let result = enumerate("branch", Scope::Prefix(None), 50, &env)
            .await
            .unwrap();
        assert_eq!(result.total, 50);
        assert_eq!(
            fs::read_to_string(env.cwd.join("calls"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let evidence =
            enrich_with_env("branch", &["b9".into(), "b45".into(), "b2".into()], &env).await;
        for (value, name) in evidence.iter().zip(["b9", "b45", "b2"]) {
            assert!(value.contains(&format!("subject-{name}")), "{value}");
        }
        let calls = fs::read_to_string(env.cwd.join("calls")).unwrap();
        assert_eq!(calls.lines().count(), 4);
        for (call, name) in calls.lines().skip(1).zip(["b9", "b45", "b2"]) {
            assert!(call.ends_with(&format!("{name} --")));
        }
    }

    #[tokio::test]
    async fn runner_drains_both_pipes_and_injects_environment_and_null_stdin() {
        let mut env = fake_git(
            r#"
[ "$GH_PROMPT_DISABLED" = 1 ] && [ "$GIT_TERMINAL_PROMPT" = 0 ] && [ "$NO_COLOR" = 1 ] || exit 2
[ "$JEVIFY_CONFIG_DIR" = "$PWD/config" ] || exit 3
if read -r line; then exit 4; fi
i=0
while [ "$i" -lt 12000 ]; do
    printf 'stdout stream\n'
    printf 'stderr stream\n' >&2
    i=$((i + 1))
done
"#,
        );
        env.config_dir = Some(env.cwd.join("config"));
        let result = run_lister(&["git".into()], &env).await.unwrap();
        assert_eq!(result.len(), 12000 * b"stdout stream\n".len());
    }

    #[tokio::test]
    async fn runner_deadline_kills_and_does_not_block_runtime() {
        let mut env = fake_git("exec /bin/sleep 2");
        env.deadline = Instant::now() + Duration::from_millis(100);
        let start = Instant::now();
        let argv = ["git".into()];
        let (result, ticks) = tokio::join!(run_lister(&argv, &env), async {
            let mut ticks = 0;
            while start.elapsed() < Duration::from_millis(80) {
                tokio::time::sleep(Duration::from_millis(5)).await;
                ticks += 1;
            }
            ticks
        });
        assert!(ticks >= 2);
        assert!(start.elapsed() < Duration::from_millis(700));
        let error = result.unwrap_err();
        assert_eq!(error.kind(), "lister_failed");
        assert!(error.to_string().contains("deadline"));
    }

    #[tokio::test]
    async fn runner_discards_partial_output_after_exit_or_inherited_pipe() {
        for body in [
            "printf 'refs/heads/x\\000\\0001\\000tip\\000\\n'; printf 'tool failed' >&2; exit 1",
            "printf 'refs/heads/x\\000\\0001\\000tip\\000\\n'; /bin/sleep 2 & exit 0",
        ] {
            let env = fake_git(body);
            let start = Instant::now();
            let error = enumerate("branch", Scope::Prefix(None), 10, &env)
                .await
                .unwrap_err();
            assert_eq!(error.kind(), "lister_failed");
            assert!(start.elapsed() < Duration::from_millis(900));
            if body.contains("exit 1") {
                assert!(error.to_string().contains("tool failed"));
            } else {
                assert!(error.to_string().contains("EOF"));
            }
        }
    }

    /// A scratch directory holding executable `name` (first on the injected PATH) and a
    /// configuration directory `config` whose `kinds.jsonl` is `recipes`.
    fn fake_tool(name: &str, body: &str, recipes: Option<&str>) -> Env {
        let dir = scratch();
        let tool = dir.join(name);
        fs::write(&tool, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(dir.join("config")).unwrap();
        if let Some(recipes) = recipes {
            fs::write(dir.join("config").join("kinds.jsonl"), recipes).unwrap();
        }
        Env {
            path: format!("{}:/usr/bin:/bin", dir.display()).into(),
            config_dir: Some(dir.join("config")),
            ..environment(&dir)
        }
    }

    fn handles(listing: &Listing) -> Vec<&OsStr> {
        listing
            .records
            .iter()
            .map(|r| r.handle.as_os_str())
            .collect()
    }

    #[test]
    fn shipped_recipes_parse_are_unique_and_match_the_registry() {
        assert_eq!(SHIPPED.lines().count(), 7);
        let recipes = shipped();
        assert_eq!(recipes.len(), 7);
        let mut names = HashSet::new();
        for recipe in recipes {
            assert!(valid_kind_name(&recipe.kind), "{}", recipe.kind);
            assert!(names.insert(recipe.kind.as_str()), "{}", recipe.kind);
            let kind = kind(&recipe.kind).unwrap();
            assert!(!kind.coded && !kind.path_kind && !kind.has_tier_two);
            assert_eq!(kind.ordered, recipe.ordered, "{}", recipe.kind);
        }
        let coded: Vec<_> = REGISTRY
            .iter()
            .filter(|k| k.coded)
            .map(|k| &k.name)
            .collect();
        assert_eq!(coded, ["-", "branch"]);
        let shipped_names: Vec<_> = recipes.iter().map(|r| r.kind.as_str()).collect();
        assert_eq!(&KINDS[coded.len()..], shipped_names.as_slice());
        assert_eq!(
            shipped_names,
            [
                "pr",
                "issue",
                "ci-run",
                "stash",
                "process",
                "container",
                "pod"
            ]
        );
        for name in ["", "Pr", "-x", "1pr", "pr_x", "pr x"] {
            assert!(!valid_kind_name(name), "{name}");
        }
        assert!(valid_kind_name("ci-run") && valid_kind_name("x-"));
    }

    #[tokio::test]
    async fn a_bad_user_line_is_recipe_invalid_with_its_number_for_any_user_kind() {
        let good = r#"{"kind":"widget","list":["widget"]}"#;
        for bad in [
            r#"{"kind":"gadget","list":["gadget"],"evidence":"x"}"#,
            r#"{"kind":"gadget","list":["gadget"],"field":1,"key":"id"}"#,
            r#"{"kind":"gadget","list":[]}"#,
            r#"{"kind":"Gadget","list":["gadget"]}"#,
            r#"{"kind":"gadget","list":["gadget"]"#,
        ] {
            let env = fake_tool("widget", "echo w", Some(&format!("{good}\n\n{bad}\n")));
            for name in ["widget", "gadget", "other"] {
                let error = lookup(name, &env).unwrap_err();
                assert_eq!(error.kind(), "recipe_invalid", "{bad}");
                assert_eq!(error.exit().code(), 6);
                assert!(error.to_string().contains("line 3"), "{error}");
            }
            let error = enumerate("widget", Scope::Prefix(None), 10, &env)
                .await
                .unwrap_err();
            assert_eq!(error.kind(), "recipe_invalid");
            assert!(catalog(&env).error.unwrap().contains("line 3"));
        }
    }

    #[tokio::test]
    async fn shipped_and_coded_kinds_never_open_the_user_file() {
        // kinds.jsonl is a directory, and a config_dir that is a file: reading either fails.
        let in_place = fake_tool("gh", "printf '[{\"number\":7,\"title\":\"fix\"}]'", None);
        fs::create_dir(in_place.config_dir.as_ref().unwrap().join("kinds.jsonl")).unwrap();
        let not_a_dir = Env {
            config_dir: Some(in_place.cwd.join("gh")),
            ..in_place.clone()
        };
        for env in [&in_place, &not_a_dir] {
            for name in ["-", "branch", "pr", "pod"] {
                assert_eq!(lookup(name, env).unwrap().unwrap().name, name);
            }
            let listing = enumerate("pr", Scope::Prefix(None), 10, env).await.unwrap();
            assert_eq!(handles(&listing), ["7"]);
            assert_eq!(lookup("widget", env).unwrap_err().kind(), "recipe_invalid");
            let catalog = catalog(env);
            assert_eq!(catalog.kinds.len(), KINDS.len());
            assert!(catalog.error.is_some());
        }
    }

    #[tokio::test]
    async fn user_recipes_resolve_but_never_shadow_and_never_come_from_the_cwd() {
        for shadow in ["branch", "pr"] {
            let env = fake_tool(
                "widget",
                "echo w",
                Some(&format!(
                    "{{\"kind\":\"widget\",\"list\":[\"widget\"]}}\n{{\"kind\":\"{shadow}\",\"list\":[\"x\"]}}\n"
                )),
            );
            let error = lookup("widget", &env).unwrap_err();
            assert_eq!(error.kind(), "recipe_invalid");
            assert!(error.to_string().contains("line 2"), "{error}");
            // The shipped kind itself is unaffected: the file is not read.
            assert!(lookup(shadow, &env).unwrap().unwrap().ordered);
        }
        let twice = fake_tool(
            "widget",
            "echo w",
            Some(
                "{\"kind\":\"widget\",\"list\":[\"a\"]}\n{\"kind\":\"widget\",\"list\":[\"b\"]}\n",
            ),
        );
        assert!(
            lookup("widget", &twice)
                .unwrap_err()
                .to_string()
                .contains("line 2")
        );
        let env = fake_tool(
            "widget",
            "[ \"$1\" = --all ] || exit 9\nprintf 'w1 first widget\\nw2 second widget\\n'",
            Some("{\"kind\":\"widget\",\"list\":[\"widget\",\"--all\"],\"field\":1}\n"),
        );
        let widget = lookup("widget", &env).unwrap().unwrap();
        assert_eq!(widget.name, "widget");
        assert!(!widget.coded && !widget.ordered);
        // The const registry never sees a user recipe.
        assert!(kind("widget").is_none());
        assert!(!KINDS.contains(&"widget"));
        let listing = enumerate("widget", Scope::Prefix(None), 1, &env)
            .await
            .unwrap();
        assert_eq!(handles(&listing), ["w1", "w2"]);
        assert_eq!(listing.records[1].evidence, "w2 second widget");
        assert_eq!((listing.total, listing.ordered), (2, false));
        let catalog = catalog(&env);
        assert_eq!(catalog.error, None);
        let last = catalog.kinds.last().unwrap();
        assert_eq!(
            (last.name.as_str(), last.origin, last.list.as_slice()),
            (
                "widget",
                "user",
                ["widget".to_owned(), "--all".to_owned()].as_slice()
            )
        );
        assert_eq!(catalog.kinds[1].list[0], "git");
        // A kinds.jsonl in the working directory is never read.
        let cwd = fake_tool("widget", "echo w", None);
        fs::write(
            cwd.cwd.join("kinds.jsonl"),
            "{\"kind\":\"widget\",\"list\":[\"widget\"]}\n",
        )
        .unwrap();
        assert!(lookup("widget", &cwd).unwrap().is_none());
        assert_eq!(
            enumerate("widget", Scope::Prefix(None), 10, &cwd)
                .await
                .unwrap_err()
                .kind(),
            "usage"
        );
        let unset = Env {
            config_dir: None,
            ..cwd
        };
        assert!(lookup("widget", &unset).unwrap().is_none());
    }

    #[tokio::test]
    async fn recipe_handles_key_field_whole_line_and_ordered_limit() {
        let recipes = [
            r#"{"kind":"lines","list":["tool","lines"],"key":"id","ordered":true}"#,
            r#"{"kind":"array","list":["tool","array"],"key":"id"}"#,
            r#"{"kind":"fields","list":["tool","fields"],"field":2}"#,
            r#"{"kind":"whole","list":["tool","whole"],"ordered":true}"#,
        ]
        .join("\n");
        let env = fake_tool(
            "tool",
            r#"case "$1" in
lines) printf '{"id":3,"title":"c"}\n{"id":2,"title":"b"}\n{"id":1,"title":"a"}\n';;
array) printf '[{"id":"x","t":"one"},{"id":"y","t":"two"}]';;
fields) printf 'a b c\nd e f\nshort\n';;
whole) printf 'newest line\nolder line\noldest line\n';;
esac"#,
            Some(&recipes),
        );
        let lines = enumerate("lines", Scope::Prefix(None), 2, &env)
            .await
            .unwrap();
        assert_eq!(handles(&lines), ["3", "2"]);
        assert_eq!((lines.total, lines.ordered), (3, true));
        assert!(lines.records[0].evidence.contains("\"title\":\"c\""));
        let array = enumerate("array", Scope::Prefix(None), 1, &env)
            .await
            .unwrap();
        assert_eq!(handles(&array), ["x", "y"]);
        assert_eq!((array.total, array.ordered), (2, false));
        assert!(array.records[1].evidence.contains("two"));
        let fields = enumerate("fields", Scope::Prefix(None), 10, &env)
            .await
            .unwrap();
        assert_eq!(handles(&fields), ["b", "e"]);
        assert_eq!(fields.records[0].evidence, "a b c");
        assert_eq!(fields.omitted, 1);
        let whole = enumerate("whole", Scope::Prefix(None), 1, &env)
            .await
            .unwrap();
        assert_eq!(handles(&whole), ["newest line"]);
        assert_eq!((whole.total, whole.ordered), (3, true));
    }

    #[tokio::test]
    async fn a_failing_or_garbled_lister_is_lister_failed_with_its_text() {
        let env = fake_tool(
            "gh",
            "echo 'To get started with GitHub CLI, please run: gh auth login' >&2\necho 'not logged in' >&2\nexit 1",
            None,
        );
        for name in ["pr", "issue", "ci-run"] {
            let error = enumerate(name, Scope::Prefix(None), 10, &env)
                .await
                .unwrap_err();
            assert_eq!(error.kind(), "lister_failed");
            assert!(error.to_string().contains("not logged in"), "{error}");
        }
        let missing = Env {
            path: "/nonexistent".into(),
            ..env
        };
        assert_eq!(
            enumerate("pr", Scope::Prefix(None), 10, &missing)
                .await
                .unwrap_err()
                .kind(),
            "lister_failed"
        );
        let garbled = fake_tool("gh", "echo 'not json'", None);
        let error = enumerate("pr", Scope::Prefix(None), 10, &garbled)
            .await
            .unwrap_err();
        assert_eq!(error.kind(), "lister_failed");
        assert!(error.to_string().contains("gh"));
    }

    #[test]
    fn runner_rejects_oversize_missing_program_expired_deadline_and_bounds_stderr() {
        let env = fake_git("printf '1234567890'; printf '1234567890' >&2");
        let error = run_lister_blocking(&["git".into()], &env, 15).unwrap_err();
        assert!(error.to_string().contains("cap"));
        let error = run_lister_blocking(&["missing-program".into()], &env, 100).unwrap_err();
        assert_eq!(error.kind(), "lister_failed");
        assert_eq!(
            run_lister_blocking(&[], &env, 100).unwrap_err().kind(),
            "lister_failed"
        );
        let expired = Env {
            deadline: Instant::now(),
            ..env
        };
        assert!(
            run_lister_blocking(&["git".into()], &expired, 100)
                .unwrap_err()
                .to_string()
                .contains("deadline")
        );
        let env = fake_git(
            "i=0; while [ \"$i\" -lt 5000 ]; do printf x >&2; i=$((i + 1)); done; printf tail >&2; exit 1",
        );
        let error = run_lister_blocking(&["git".into()], &env, OUTPUT_CAP).unwrap_err();
        assert!(error.to_string().ends_with("tail"));
        assert!(error.to_string().len() < 4200);
    }
}
