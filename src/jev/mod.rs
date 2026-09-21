pub mod cache;
pub mod classifier;
pub mod client;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const UNKNOWN_MODEL: &str = "unknown";

fn normalize_model(model: &str) -> &str {
    if model.trim().is_empty() {
        UNKNOWN_MODEL
    } else {
        model
    }
}

fn unknown_model() -> String {
    UNKNOWN_MODEL.to_owned()
}

fn deserialize_model<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let model = String::deserialize(deserializer)?;
    Ok(normalize_model(&model).to_owned())
}

#[derive(Serialize, Clone, Debug)]
pub struct NoulCriteria {
    #[serde(rename = "true")]
    pub yes: String,
    #[serde(rename = "false")]
    pub no: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    Noul {
        instructions: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        criteria: Option<NoulCriteria>,
    },
    Choice {
        instructions: String,
        criteria: BTreeMap<String, Option<String>>,
    },
}

impl Question {
    pub fn noul(instructions: impl Into<String>) -> Self {
        Self::Noul {
            instructions: instructions.into(),
            criteria: None,
        }
    }
    pub fn noul_with(
        instructions: impl Into<String>,
        yes: impl Into<String>,
        no: impl Into<String>,
    ) -> Self {
        Self::Noul {
            instructions: instructions.into(),
            criteria: Some(NoulCriteria {
                yes: yes.into(),
                no: no.into(),
            }),
        }
    }
    pub fn choice(
        instructions: impl Into<String>,
        criteria: BTreeMap<String, Option<String>>,
    ) -> Self {
        Self::Choice {
            instructions: instructions.into(),
            criteria,
        }
    }
}

/// BTreeMap keeps request bodies byte-stable, which makes cache keys stable.
pub type Questions = BTreeMap<String, Question>;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Answer {
    #[serde(default)]
    pub noul: Option<f64>,
    #[serde(default)]
    pub choice: Option<String>,
    #[serde(default)]
    pub probabilities: Option<BTreeMap<String, f64>>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

/// Lenient on purpose: the API is in early access, and a renamed or missing `usage`/`model`
/// field must degrade `meta`, not fail every command with exit 4. `answers` stays required.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Response {
    #[serde(default = "unknown_model", deserialize_with = "deserialize_model")]
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    #[serde(default)]
    pub usage: Usage,
}

impl Response {
    /// Every contributing model must be Jev; missing provenance is not sufficient.
    pub fn all_jev(&self) -> bool {
        all_jev(&self.model)
    }

    /// Validate only decision-bearing fields; unused answers and metadata may evolve.
    pub fn validate(&self, questions: &Questions) -> Result<(), crate::exit::JevifyError> {
        for (id, question) in questions {
            let answer = self.answers.get(id).ok_or_else(|| {
                crate::exit::JevifyError::Protocol(format!("missing answer `{id}`"))
            })?;
            if answer.confidence.is_some_and(|p| !valid_probability(p)) {
                return Err(crate::exit::JevifyError::Protocol(format!(
                    "invalid confidence for `{id}`"
                )));
            }
            match question {
                Question::Noul { .. } => {
                    if !answer.noul.is_some_and(valid_probability) {
                        return Err(crate::exit::JevifyError::Protocol(format!(
                            "invalid noul for `{id}`"
                        )));
                    }
                }
                Question::Choice { criteria, .. } => {
                    let valid = answer
                        .choice
                        .as_ref()
                        .is_some_and(|label| criteria.contains_key(label))
                        && answer.probabilities.as_ref().is_some_and(|scores| {
                            scores.len() == criteria.len()
                                && criteria.keys().all(|label| {
                                    scores.get(label).copied().is_some_and(valid_probability)
                                })
                        });
                    if !valid {
                        return Err(crate::exit::JevifyError::Protocol(format!(
                            "invalid choice for `{id}`"
                        )));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn noul(&self, id: &str) -> Result<f64, crate::exit::JevifyError> {
        self.answers
            .get(id)
            .and_then(|a| a.noul)
            .ok_or_else(|| crate::exit::JevifyError::Protocol(format!("missing noul `{id}`")))
    }
    pub fn probs(&self, id: &str) -> Result<&BTreeMap<String, f64>, crate::exit::JevifyError> {
        self.answers
            .get(id)
            .and_then(|a| a.probabilities.as_ref())
            .ok_or_else(|| crate::exit::JevifyError::Protocol(format!("missing choice `{id}`")))
    }
}

pub(crate) fn valid_probability(p: f64) -> bool {
    p.is_finite() && (0.0..=1.0).contains(&p)
}

/// The common provenance check for action guards and output status lines.
pub fn all_jev(model: &str) -> bool {
    !model.is_empty() && model.split(", ").all(|part| part.starts_with("jev"))
}

pub(crate) fn join_models(into: &mut String, models: &str) {
    for model in models.split(", ").map(normalize_model) {
        if !into.split(", ").any(|seen| seen == model) {
            if !into.is_empty() {
                into.push_str(", ");
            }
            into.push_str(model);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_union_is_stable_and_guard_checks_every_part() {
        let mut model = String::new();
        join_models(&mut model, "other-model, jev-fake");
        join_models(&mut model, "jev-fake, jev-next");
        assert_eq!(model, "other-model, jev-fake, jev-next");
        assert!(!all_jev(&model));
        assert!(all_jev("jev-fake, jev-next"));
        assert!(!all_jev(""));
        assert!(!all_jev("jev-fake, "));
        join_models(&mut model, "unknown, jev-fake, unknown");
        assert_eq!(model, "other-model, jev-fake, jev-next, unknown");
        for model in ["unknown", "unknown, jev-fake", "jev-fake, unknown"] {
            assert!(!all_jev(model));
        }
    }
    #[test]
    fn decision_validation_requires_complete_finite_member_scores() {
        let qs = [(
            "q".into(),
            Question::choice(
                "pick",
                [("L000".into(), None), ("NONE".into(), None)].into(),
            ),
        )]
        .into();
        for value in [
            serde_json::json!({"choice":"L000","probabilities":{"L000":0.9}}),
            serde_json::json!({"probabilities":{"L000":0.9,"NONE":0.1}}),
            serde_json::json!({"choice":"L000","probabilities":{"L000":-0.1,"NONE":0.1}}),
            serde_json::json!({"choice":"é","probabilities":{"L000":0.9,"NONE":0.1}}),
        ] {
            let r: Response =
                serde_json::from_value(serde_json::json!({"answers":{"q":value}})).unwrap();
            assert!(r.validate(&qs).is_err());
        }
        let r: Response = serde_json::from_value(serde_json::json!({"answers":{"q":{"choice":"L000","probabilities":{"L000":0.9,"NONE":0.1},"future":42}},"future":true})).unwrap();
        assert!(r.validate(&qs).is_ok());
        assert!(!valid_probability(f64::NAN));
        assert!(!valid_probability(f64::INFINITY));
    }
    #[test]
    fn question_serializes_to_api_shape() {
        let mut crit = BTreeMap::new();
        crit.insert("L000".to_string(), None);
        crit.insert("NONE".to_string(), Some("nothing fits".to_string()));
        let q = Question::choice("Which line?", crit);
        let v = serde_json::to_value(&q).unwrap();
        assert_eq!(v["type"], "choice");
        assert_eq!(v["criteria"]["L000"], serde_json::Value::Null);
        let n =
            serde_json::to_value(Question::noul_with("Is it?", "yes means", "no means")).unwrap();
        assert_eq!(n["type"], "noul");
        assert_eq!(n["criteria"]["true"], "yes means");
    }
    #[test]
    fn response_parses_choice_and_noul() {
        let r: Response = serde_json::from_str(r#"{"model":"jev-1.13.0","answers":{"pick":{"type":"choice","choice":"L001","probabilities":{"L000":0.1,"L001":0.9},"confidence":0.8},"any":{"type":"noul","noul":0.97},"future":{"type":"score","score":3,"legend":{"1":"low"}}},"usage":{"input_tokens":120,"output_tokens":20}}"#).unwrap();
        assert_eq!(r.noul("any").unwrap(), 0.97);
        assert_eq!(r.probs("pick").unwrap()["L001"], 0.9);
        // Forward compatibility: an answer type or field jevify does not know parses into an
        // `Answer` of `None`s; only reading it as a noul/choice fails, never the whole response.
        assert!(r.answers.contains_key("future"));
        assert_eq!(r.probs("future").unwrap_err().exit().code(), 4);
    }
    #[test]
    fn response_tolerates_missing_model_and_usage() {
        let r: Response = serde_json::from_str(r#"{"answers":{"q":{"noul":0.5}}}"#).unwrap();
        assert_eq!(r.noul("q").unwrap(), 0.5);
        assert_eq!(r.usage.input_tokens, 0);
        assert_eq!(r.model, "unknown");
        for model in ["", "  ", "\t\n"] {
            let r: Response =
                serde_json::from_value(serde_json::json!({"model": model, "answers": {}})).unwrap();
            assert_eq!(r.model, "unknown");
        }
        assert!(r.noul("missing").unwrap_err().exit().code() == 4);
    }
}
