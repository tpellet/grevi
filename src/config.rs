use crate::cli::GlobalOpts;
use crate::exit::GreviError;
use crate::jev::client::Stats;
use crate::output::Meta;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Which service answers the questions. classifier.dev translates Noul into a two-label
/// Choice; its relative scores and TypeSafe's absolute Noul have different semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Typesafe,
    Classifier,
}

impl Backend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Typesafe => "typesafe",
            Self::Classifier => "classifier",
        }
    }
    pub const fn default_base_url(self) -> &'static str {
        match self {
            Self::Typesafe => "https://api.typesafe.ai",
            Self::Classifier => "https://classifier.dev",
        }
    }
    /// How many items one Choice may offer, NONE excluded. TypeSafe accepts 255 options and
    /// grevi windows at 200; classifier.dev caps a dimension at 100 labels, so NONE takes the
    /// hundredth slot. Only the number of windows changes, never what a window asks.
    pub const fn window(self) -> usize {
        match self {
            Self::Typesafe => 200,
            Self::Classifier => 99,
        }
    }
    /// Character budget for the state a caller may build. classifier.dev rejects an input over
    /// 32,000 characters (`input_too_long`); 30,000 leaves room for the JSON around the items.
    pub const fn max_state_chars(self) -> usize {
        match self {
            Self::Typesafe => usize::MAX,
            Self::Classifier => 30_000,
        }
    }
    /// 8 in flight is ~20 req/s, under TypeSafe's 1,200/min. classifier.dev is free and shared
    /// per IP, so grevi stays at 4 there by default.
    pub const fn default_concurrency(self) -> usize {
        match self {
            Self::Typesafe => 8,
            Self::Classifier => 4,
        }
    }
}

pub struct Config {
    pub backend: Backend,
    pub key: Option<String>,
    /// Read lazily by `api_key`: `capabilities`, `init` and `robot-docs` need no key, so a bad
    /// path must not break them.
    pub key_file: Option<PathBuf>,
    pub base_url: String,
    pub model: String,
    pub threshold: f64,
    pub concurrency: usize,
    pub cache_dir: Option<PathBuf>,
    pub price_per_mtok: f64,
    pub stats: Arc<Stats>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn parse<T: std::str::FromStr>(name: &str, default: T) -> Result<T, GreviError> {
    match env(name) {
        None => Ok(default),
        Some(v) => v
            .parse()
            .map_err(|_| GreviError::Usage(format!("{name}={v} is not valid"))),
    }
}

impl Config {
    pub fn load(g: &GlobalOpts) -> Result<Self, GreviError> {
        let threshold = g.threshold.unwrap_or(0.5);
        if !(0.0..=1.0).contains(&threshold) {
            return Err(GreviError::Usage(format!(
                "threshold {threshold} must be within 0..=1"
            )));
        }
        let cache_dir = if g.no_cache || env("GREVI_NO_CACHE").is_some() {
            None
        } else if let Some(d) = env("GREVI_CACHE_DIR") {
            Some(PathBuf::from(d))
        } else {
            directories::ProjectDirs::from("", "", "grevi").map(|p| p.cache_dir().to_path_buf())
        };
        let key = env("TYPESAFE_API_KEY");
        let key_file = env("TYPESAFE_API_KEY_FILE").map(PathBuf::from);
        let backend = match env("GREVI_BACKEND").as_deref() {
            None => {
                // No key, no prompt and no signup path: grevi answers out of the box through
                // classifier.dev, and uses your own TypeSafe quota as soon as a key is there.
                if key.is_some() || key_file.is_some() {
                    Backend::Typesafe
                } else {
                    Backend::Classifier
                }
            }
            Some("typesafe") => Backend::Typesafe,
            Some("classifier") => Backend::Classifier,
            Some(other) => {
                return Err(GreviError::Usage(format!(
                    "GREVI_BACKEND={other} is not valid; use typesafe or classifier"
                )));
            }
        };
        if backend == Backend::Classifier && g.model.is_some() {
            return Err(GreviError::Usage(
                "--model/GREVI_MODEL requires the typesafe backend; classifier chooses its model"
                    .into(),
            ));
        }
        Ok(Self {
            backend,
            key,
            key_file,
            base_url: env("GREVI_BASE_URL")
                .unwrap_or_else(|| backend.default_base_url().into())
                .trim_end_matches('/')
                .to_string(),
            // TypeSafe requests pin this model. classifier.dev chooses its own model and the
            // resolved response identity is reported in meta.model.
            model: g.model.clone().unwrap_or_else(|| "jev-1.13.0".into()),
            threshold,
            // 16 in flight at ~0.4 s each is ~40 req/s, twice the 1,200/min budget; 8 stays under it.
            concurrency: parse("GREVI_CONCURRENCY", backend.default_concurrency())?.max(1),
            cache_dir,
            price_per_mtok: parse(
                "GREVI_PRICE_PER_MTOK",
                if backend == Backend::Classifier {
                    0.0
                } else {
                    0.042f64
                },
            )?,
            stats: Arc::new(Stats::default()),
        })
    }

    /// The key from `TYPESAFE_API_KEY`, else the trimmed contents of `TYPESAFE_API_KEY_FILE`.
    pub fn api_key(&self) -> Result<String, GreviError> {
        if let Some(k) = &self.key {
            return Ok(k.clone());
        }
        let Some(p) = &self.key_file else {
            return Err(GreviError::MissingKey);
        };
        let k = std::fs::read_to_string(p).map_err(|e| {
            GreviError::Input(format!("TYPESAFE_API_KEY_FILE {}: {e}", p.display()))
        })?;
        let k = k.trim().to_string();
        if k.is_empty() {
            return Err(GreviError::MissingKey);
        }
        Ok(k)
    }

    pub fn meta(&self) -> Meta {
        let s = &self.stats;
        let mut telemetry = s.telemetry();
        let usage = &telemetry.usage.input_tokens;
        let tokens = usage.reported_subtotal;
        let cost = tokens as f64 * self.price_per_mtok / 1_000_000.0;
        let cost_complete = usage.complete || self.price_per_mtok == 0.0;
        let input_tokens = usage.complete.then_some(tokens);
        telemetry.cost_estimate = Some(crate::output::CostEstimate {
            basis: if self.backend == Backend::Classifier && self.price_per_mtok == 0.0 {
                "free_service"
            } else {
                "configured_input_token_price"
            },
            input_price_per_mtok: self.price_per_mtok,
            reported_input_subtotal_usd: cost,
            complete: cost_complete,
        });
        Meta {
            backend: self.backend.as_str(),
            model: s.model.lock().unwrap().clone(),
            elapsed_ms: 0,
            requests: telemetry.inference_posts.attempted,
            cache_hits: s.cache_hits.load(Ordering::Relaxed),
            input_tokens,
            cost_usd: cost_complete.then_some(cost),
            threshold: self.threshold,
            request_id: s.request_id.lock().unwrap().clone(),
            telemetry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Built literally: unit tests inside src/ never touch the process environment (AGENTS.md).
    fn cfg(key: Option<&str>, key_file: Option<&str>) -> Config {
        Config {
            backend: Backend::Typesafe,
            key: key.map(String::from),
            key_file: key_file.map(PathBuf::from),
            base_url: String::new(),
            model: String::new(),
            threshold: 0.5,
            concurrency: 8,
            cache_dir: None,
            price_per_mtok: 0.042,
            stats: Arc::new(Stats::default()),
        }
    }
    #[test]
    fn key_is_read_lazily_and_a_bad_file_is_an_input_error() {
        assert_eq!(cfg(Some("k"), None).api_key().unwrap(), "k");
        assert_eq!(cfg(None, None).api_key().unwrap_err().exit().code(), 5);
        assert_eq!(
            cfg(None, Some("/nonexistent/grevi-key"))
                .api_key()
                .unwrap_err()
                .exit()
                .code(),
            6
        );
    }
    #[test]
    fn meta_without_inference_has_zero_usage_and_names_the_backend() {
        let c = cfg(None, None);
        assert_eq!(c.meta().input_tokens, Some(0));
        assert_eq!(c.meta().cost_usd, Some(0.0));
        assert_eq!(c.meta().backend, "typesafe");
    }
    #[test]
    fn backend_limits_fit_each_api() {
        // classifier.dev caps a dimension at 100 labels; NONE takes the hundredth slot.
        assert_eq!(Backend::Classifier.window() + 1, 100);
        assert_eq!(Backend::Typesafe.window(), 200);
        // and its input at 32,000 characters.
        assert!(Backend::Classifier.max_state_chars() < 32_000);
        assert_eq!(Backend::Typesafe.max_state_chars(), usize::MAX);
        assert!(
            Backend::Classifier.default_concurrency() < Backend::Typesafe.default_concurrency()
        );
        assert_eq!(
            Backend::Classifier.default_base_url(),
            "https://classifier.dev"
        );
        assert_eq!(Backend::Typesafe.as_str(), "typesafe");
    }
}
