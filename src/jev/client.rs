use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64};

#[derive(Default, Debug)]
pub struct Stats {
    pub requests: AtomicU32,
    pub cache_hits: AtomicU32,
    pub input_tokens: AtomicU64,
    pub model: Mutex<Option<String>>,
    /// `x-typesafe-request-id` of the last response seen, success or failure (surfaced in `meta`).
    pub request_id: Mutex<Option<String>>,
}
