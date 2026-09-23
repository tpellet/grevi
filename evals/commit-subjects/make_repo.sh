#!/usr/bin/env bash
# Build the scratch repository of lying subjects.
#
# Usage: make_repo.sh <directory>
#
# Ten pairs, interleaved. Each pair is a documentation-only commit whose subject
# announces a change ("fix: payment gateway timeout") and a code commit whose
# subject says nothing ("chore: tidy imports in payments") and which holds the
# change the other one announces. The target of every case is the code commit.
# Commits are unsigned and dated from a fixed clock, so the shas are stable.
set -euo pipefail

dir=${1:?usage: make_repo.sh <directory>}
[ -e "$dir" ] && { echo "refusing to write over $dir" >&2; exit 1; }
mkdir -p "$dir/src" "$dir/docs"
cd "$dir"
git init -q -b main .
clock=1700000000

commit() { # commit <subject>
  clock=$((clock + 3600))
  git add -A
  GIT_AUTHOR_NAME=eval GIT_AUTHOR_EMAIL=eval@example.com \
  GIT_COMMITTER_NAME=eval GIT_COMMITTER_EMAIL=eval@example.com \
  GIT_AUTHOR_DATE="$clock +0000" GIT_COMMITTER_DATE="$clock +0000" \
  git -c commit.gpgsign=false commit -q -m "$1"
}

seed() { printf '%s\n' "$2" > "$1"; }

# --- seed files, so every later commit is a modification, not an addition ---
for m in payments webhooks users auth search uploads logging queue ratelimit billing; do
  printf 'pub fn %s() {\n    // placeholder\n}\n' "$m" > "src/$m.rs"
  printf '# %s\n\nPlaceholder.\n' "$m" > "docs/$m.md"
done
printf '# service\n' > README.md
commit "init: scaffold the service"

# --- pair 1 ---
seed docs/payments.md '# payments

The gateway call gives up after five seconds.'
commit "fix: payment gateway timeout"
seed src/payments.rs 'use std::time::Duration;

pub const GATEWAY_TIMEOUT: Duration = Duration::from_secs(5);

pub fn charge() {}'
commit "chore: tidy imports in payments"

# --- pair 2 ---
seed docs/webhooks.md '# webhooks

A failed delivery is retried with an exponential backoff.'
commit "feat: retry failed webhooks with backoff"
seed src/webhooks.rs 'pub fn dispatch(payload: &str) -> bool {
    let mut delay = 1;
    for _ in 0..5 {
        if send(payload) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_secs(delay));
        delay *= 2;
    }
    false
}

fn send(_payload: &str) -> bool { true }'
commit "style: rename a local in dispatch"

# --- pair 3 ---
seed docs/users.md '# users

Repeated lookups of the same user are served from memory.'
commit "perf: cache the user lookup"
seed src/users.rs 'use std::collections::HashMap;
use std::sync::Mutex;

static CACHE: Mutex<Option<HashMap<u64, String>>> = Mutex::new(None);

pub fn lookup(id: u64) -> String {
    let mut guard = CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(hit) = map.get(&id) {
        return hit.clone();
    }
    let name = fetch(id);
    map.insert(id, name.clone());
    name
}

fn fetch(id: u64) -> String { format!("user{id}") }'
commit "refactor: shuffle helper order"

# --- pair 4 ---
seed docs/auth.md '# auth

A session past its expiry is refused.'
commit "fix: reject expired sessions"
seed src/auth.rs 'pub struct Session { pub expires_at: u64 }

pub fn authorize(session: &Session, now: u64) -> bool {
    if now >= session.expires_at {
        return false;
    }
    true
}'
commit "chore: bump comment year"

# --- pair 5 ---
seed docs/search.md '# search

Results are returned one page at a time.'
commit "feat: paginate the search endpoint"
seed src/search.rs 'pub fn search(q: &str, limit: usize, offset: usize) -> Vec<String> {
    let all = matches_for(q);
    all.into_iter().skip(offset).take(limit).collect()
}

fn matches_for(_q: &str) -> Vec<String> { Vec::new() }'
commit "test: tidy fixture names"

# --- pair 6 ---
seed docs/uploads.md '# uploads

An upload with no bytes is rejected instead of crashing.'
commit "fix: crash on empty upload"
seed src/uploads.rs 'pub fn receive(body: &[u8]) -> Result<usize, &str> {
    if body.is_empty() {
        return Err("empty body");
    }
    Ok(body.len())
}'
commit "docs: clarify a comment in uploads"

# --- pair 7 ---
seed docs/logging.md '# logging

Every line the service writes is a JSON object.'
commit "feat: structured JSON logging"
seed src/logging.rs 'pub fn log(level: &str, message: &str) {
    println!("{{\"level\":\"{level}\",\"message\":\"{message}\"}}");
}'
commit "chore: sort the module list"

# --- pair 8 ---
seed docs/queue.md '# queue

Workers no longer race for the same job.'
commit "fix: race in the job queue"
seed src/queue.rs 'use std::sync::Mutex;

pub struct Queue { jobs: Mutex<Vec<String>> }

impl Queue {
    pub fn take(&self) -> Option<String> {
        self.jobs.lock().unwrap().pop()
    }
}'
commit "style: whitespace in queue"

# --- pair 9 ---
seed docs/ratelimit.md '# ratelimit

Each API key gets its own budget of requests.'
commit "feat: rate limit by API key"
seed src/ratelimit.rs 'pub struct Bucket { pub tokens: u32, pub capacity: u32 }

pub fn allow(bucket: &mut Bucket) -> bool {
    if bucket.tokens == 0 {
        return false;
    }
    bucket.tokens -= 1;
    true
}

pub fn refill(bucket: &mut Bucket, n: u32) {
    bucket.tokens = (bucket.tokens + n).min(bucket.capacity);
}'
commit "chore: update copyright header"

# --- pair 10 ---
seed docs/billing.md '# billing

Money is rounded to the cent, halves to the even cent.'
commit "fix: wrong currency rounding"
seed src/billing.rs 'pub fn to_cents(amount: f64) -> i64 {
    let scaled = amount * 100.0;
    let floor = scaled.floor();
    let frac = scaled - floor;
    let mut cents = floor as i64;
    if frac > 0.5 || (frac == 0.5 && cents % 2 != 0) {
        cents += 1;
    }
    cents
}'
commit "refactor: extract a constant"

git log --format='%h%x09%s' --reverse
