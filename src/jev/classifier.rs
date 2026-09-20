//! classifier.dev as a second backend.
//!
//! classifier.dev is a free, keyless front end to the same Jev model grevi asks through
//! TypeSafe (its upstream error codes are literally `typesafe_<status>`, and a live call
//! answers `model: "jev-1.13.0"`). So this module is a wire translation, not a second
//! design: grevi's questions go out as *dimensions* on one item and the answers come back
//! into the same [`Response`] every verb already reads.
//!
//! What the two wire formats do not share, and how it is bridged:
//!
//! * Jev takes `criteria{option: description}`; a dimension takes a flat label list plus one
//!   free-text `instructions`. Descriptions are folded into the instructions, one line each.
//! * A Jev Noul is an absolute yes/no probability; the closest dimension is a two-label
//!   choice between its `true` and `false` criteria, and P(true) is that label's score. This
//!   is the one place where the question asked is not literally the same: a dimension weighs
//!   the two sides against each other where a Noul answers in the absolute.
//! * A request carries at most 20 dimensions, so a bigger `Questions` map is split into
//!   several requests by the client; this module maps one chunk.

use super::{Answer, Question, Questions, Response, Usage};
use crate::exit::GreviError;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Dimensions per request (`maxProperties: 20` on `ClassifyRequest.dimensions`).
pub const MAX_DIMENSIONS: usize = 20;
/// Labels per dimension (`maxItems: 100`).
pub const MAX_LABELS: usize = 100;
/// Characters per input (`maxLength: 32000` on `items[]`).
pub const MAX_INPUT_CHARS: usize = 32_000;
/// Characters per dimension's `instructions` (`maxLength: 4000`).
const MAX_INSTRUCTIONS_CHARS: usize = 4_000;
/// The two labels a Noul becomes when its criteria cannot be labels themselves.
const YES: &str = "yes";
const NO: &str = "no";
/// Label length limit (`maxLength: 200` on a dimension's labels).
const MAX_LABEL_CHARS: usize = 200;

/// The two labels a Noul is asked as, `true` first. A Noul's criteria are two sentences
/// ("this hunk implements the described work" / "this hunk is about something else"), and
/// classifier.dev answers a semantic label better than a bare yes/no — its own guidance is
/// that "'urgent bug' classifies better than 'p0'". Criteria that cannot be labels (absent,
/// equal, blank, or past the 200-character limit) fall back to `yes`/`no`, with the sentences
/// in the instructions instead.
fn noul_labels(criteria: Option<&super::NoulCriteria>) -> (String, String) {
    let usable =
        |s: &str| !s.trim().is_empty() && s.chars().count() <= MAX_LABEL_CHARS && !s.contains('\n');
    match criteria {
        Some(c) if usable(&c.yes) && usable(&c.no) && c.yes != c.no => {
            (c.yes.clone(), c.no.clone())
        }
        _ => (YES.to_string(), NO.to_string()),
    }
}

/// The state as the one text classifier.dev classifies. A string state is its own text (that
/// is what `is` sends); anything else is compact JSON. Clipped to the API's input limit so an
/// oversized state is truncated the way grevi truncates everywhere else, not rejected.
pub fn item_text(state: &serde_json::Value) -> String {
    let raw = match state {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    crate::tournament::clip(&raw, MAX_INPUT_CHARS)
}

/// One question as a dimension definition: the labels it may answer with, and the instructions
/// that carry everything Jev would read from `criteria`.
fn dimension(q: &Question) -> serde_json::Value {
    let (labels, instructions) = match q {
        Question::Noul {
            instructions,
            criteria,
        } => {
            let (yes, no) = noul_labels(criteria.as_ref());
            let mut text = instructions.clone();
            if yes == YES {
                // The fallback labels say nothing on their own, so the criteria (when there
                // are any) go into the instructions.
                text.push_str(&match criteria {
                    Some(c) => format!("\n{YES} — {}\n{NO} — {}", c.yes, c.no),
                    None => format!("\n{YES} — the answer is yes\n{NO} — the answer is no"),
                });
            }
            (vec![yes, no], text)
        }
        Question::Choice {
            instructions,
            criteria,
        } => {
            let mut text = instructions.clone();
            for (label, desc) in criteria {
                if let Some(d) = desc {
                    text.push_str(&format!("\n{label} — {d}"));
                }
            }
            (criteria.keys().cloned().collect(), text)
        }
    };
    serde_json::json!({
        "labels": labels,
        "instructions": crate::tournament::clip(&instructions, MAX_INSTRUCTIONS_CHARS),
    })
}

/// The body of one `POST /v1/classify`: one item, one dimension per question.
pub fn request_body(state: &serde_json::Value, questions: &Questions) -> serde_json::Value {
    let dimensions: serde_json::Map<String, serde_json::Value> = questions
        .iter()
        .map(|(id, q)| (id.clone(), dimension(q)))
        .collect();
    serde_json::json!({ "items": [item_text(state)], "dimensions": dimensions })
}

#[derive(Deserialize, Debug, Default)]
struct DimensionResult {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    scores: Option<BTreeMap<String, f64>>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct Item {
    #[serde(default)]
    dimensions: BTreeMap<String, DimensionResult>,
}

/// Lenient for the same reason as [`Response`]: a field grevi does not read must not fail a
/// command. `results` stays required.
#[derive(Deserialize, Debug)]
struct ClassifyResponse {
    #[serde(default)]
    model: String,
    results: Vec<Item>,
}

/// The classifier.dev answer for one chunk, read back into grevi's [`Response`].
///
/// A Noul's probability is P(yes) from `scores`; with no scores (the API documents them as
/// nullable) the verdict's own `confidence` stands in, mirrored for a `no`. A Choice keeps its
/// scores as probabilities; with no scores there is no distribution to rank items by, which is
/// a protocol error (exit 4), never a silently invented one.
pub fn parse(body: &[u8], questions: &Questions) -> Result<Response, GreviError> {
    let parsed: ClassifyResponse =
        serde_json::from_slice(body).map_err(|e| GreviError::Protocol(e.to_string()))?;
    let item = parsed
        .results
        .into_iter()
        .next()
        .ok_or_else(|| GreviError::Protocol("classify returned no result".into()))?;
    let mut answers = BTreeMap::new();
    let mut model = parsed.model;
    for (id, q) in questions {
        let Some(r) = item.dimensions.get(id) else {
            continue;
        };
        if let Some(m) = &r.model {
            model = m.clone();
        }
        let answer = match q {
            Question::Noul { criteria, .. } => {
                let (yes, no) = noul_labels(criteria.as_ref());
                let p = match (&r.scores, &r.label, r.confidence) {
                    (Some(s), _, _) => s
                        .get(&yes)
                        .copied()
                        .ok_or_else(|| GreviError::Protocol(format!("no yes score for `{id}`")))?,
                    (None, Some(label), Some(c)) if *label == yes => c,
                    (None, Some(label), Some(c)) if *label == no => 1.0 - c,
                    _ => {
                        return Err(GreviError::Protocol(format!(
                            "no probability for `{id}`; classifier.dev returned neither scores nor a confident verdict"
                        )));
                    }
                };
                Answer {
                    noul: Some(p),
                    ..Answer::default()
                }
            }
            Question::Choice { .. } => {
                Answer {
                    choice: r.label.clone(),
                    probabilities: Some(r.scores.clone().ok_or_else(|| {
                        GreviError::Protocol(format!("no scores for choice `{id}`"))
                    })?),
                    confidence: r.confidence,
                    ..Answer::default()
                }
            }
        };
        answers.insert(id.clone(), answer);
    }
    Ok(Response {
        model,
        answers,
        // classifier.dev counts classifications, not tokens: it is free, and `meta.cost_usd`
        // stays 0 because nothing was spent.
        usage: Usage::default(),
    })
}

/// The readable part of a classifier.dev error body, `{"error": "...", "code": "..."}`.
pub fn error_message(text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let msg = v.get("error")?.as_str()?;
    Some(match v.get("code").and_then(|c| c.as_str()) {
        Some(code) => format!("{code}: {msg}"),
        None => msg.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jev::Question;

    fn questions() -> Questions {
        let mut crit: BTreeMap<String, Option<String>> =
            (0..2).map(|i| (format!("L{i:03}"), None)).collect();
        crit.insert("NONE".into(), Some("nothing fits".into()));
        let mut qs = Questions::new();
        qs.insert("pick".into(), Question::choice("Which line?", crit));
        qs.insert(
            "any".into(),
            Question::noul_with("Is any of them it?", "one matches", "none matches"),
        );
        qs
    }

    #[test]
    fn a_choice_becomes_a_dimension_whose_labels_are_its_options() {
        let body = request_body(&serde_json::json!({ "items": ["a", "b"] }), &questions());
        let pick = &body["dimensions"]["pick"];
        assert_eq!(
            pick["labels"],
            serde_json::json!(["L000", "L001", "NONE"]),
            "options keep their order and NONE is one of them"
        );
        // A description with no field of its own is folded into the instructions.
        let instr = pick["instructions"].as_str().unwrap();
        assert!(
            instr.starts_with("Which line?") && instr.contains("NONE — nothing fits"),
            "{instr}"
        );
        // A Noul's criteria are the labels themselves: classifier.dev reads a semantic label
        // better than a bare yes/no, and the `true` side comes first.
        let any = &body["dimensions"]["any"];
        assert_eq!(
            any["labels"],
            serde_json::json!(["one matches", "none matches"])
        );
        assert_eq!(any["instructions"], "Is any of them it?");
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_noul_without_usable_criteria_falls_back_to_yes_and_no() {
        let mut qs = Questions::new();
        qs.insert("q".into(), Question::noul("Is it?"));
        // Criteria too long to be labels, and criteria that say the same thing, fall back too.
        qs.insert(
            "long".into(),
            Question::noul_with("Is it?", "y".repeat(201), "n"),
        );
        qs.insert("same".into(), Question::noul_with("Is it?", "x", "x"));
        let body = request_body(&serde_json::Value::String("text".into()), &qs);
        for id in ["q", "long", "same"] {
            let d = &body["dimensions"][id];
            assert_eq!(d["labels"], serde_json::json!(["yes", "no"]), "{id}");
            let instr = d["instructions"].as_str().unwrap();
            assert!(
                instr.contains("yes — ") && instr.contains("no — "),
                "{id}: {instr}"
            );
        }
        // A string state travels as itself, not as a quoted JSON string.
        assert_eq!(body["items"][0], "text");
    }

    #[test]
    fn instructions_and_input_are_clipped_to_the_api_limits() {
        let mut qs = Questions::new();
        qs.insert("q".into(), Question::noul("x".repeat(9_000)));
        let body = request_body(
            &serde_json::Value::String("y".repeat(MAX_INPUT_CHARS * 2)),
            &qs,
        );
        assert_eq!(
            body["dimensions"]["q"]["instructions"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            MAX_INSTRUCTIONS_CHARS + 1, // clip appends an ellipsis
        );
        assert_eq!(
            body["items"][0].as_str().unwrap().chars().count(),
            MAX_INPUT_CHARS + 1
        );
    }

    #[test]
    fn scores_become_probabilities_and_p_yes() {
        let body = br#"{"model":"jev-1.13.0","results":[{"dimensions":{
            "pick":{"label":"L001","confidence":0.97,"scores":{"L000":0.0,"L001":0.98,"NONE":0.02},"model":"jev-1.13.0","ms":379},
            "any":{"label":"one matches","confidence":0.99,"scores":{"one matches":0.93,"none matches":0.07},"model":"jev-1.13.0","ms":379}}}],
            "usage":{"classifications":2}}"#;
        let r = parse(body, &questions()).unwrap();
        assert_eq!(r.model, "jev-1.13.0");
        assert_eq!(r.probs("pick").unwrap()["L001"], 0.98);
        assert_eq!(r.answers["pick"].choice.as_deref(), Some("L001"));
        assert_eq!(r.noul("any").unwrap(), 0.93);
        // Free: no tokens, so no cost.
        assert_eq!(r.usage.input_tokens, 0);
    }

    #[test]
    fn a_noul_without_scores_falls_back_to_the_verdict_confidence() {
        let mut qs = Questions::new();
        qs.insert("q".into(), Question::noul("Is it?"));
        let yes = parse(
            br#"{"results":[{"dimensions":{"q":{"label":"yes","confidence":0.8,"scores":null}}}]}"#,
            &qs,
        )
        .unwrap();
        assert_eq!(yes.noul("q").unwrap(), 0.8);
        let no = parse(
            br#"{"results":[{"dimensions":{"q":{"label":"no","confidence":0.8,"scores":null}}}]}"#,
            &qs,
        )
        .unwrap();
        assert!((no.noul("q").unwrap() - 0.2).abs() < 1e-9);
        // Neither scores nor a confidence is a protocol error, not an invented probability.
        let none = parse(
            br#"{"results":[{"dimensions":{"q":{"label":"yes","confidence":null,"scores":null}}}]}"#,
            &qs,
        );
        assert_eq!(none.unwrap_err().exit().code(), 4);
    }

    #[test]
    fn missing_scores_on_a_choice_and_missing_results_are_protocol_errors() {
        assert_eq!(
            parse(
                br#"{"results":[{"dimensions":{"pick":{"label":"L001","confidence":0.9,"scores":null},"any":{"label":"one matches","scores":{"one matches":1.0,"none matches":0.0}}}}]}"#,
                &questions()
            )
            .unwrap_err()
            .exit()
            .code(),
            4
        );
        assert_eq!(
            parse(br#"{"results":[]}"#, &questions())
                .unwrap_err()
                .exit()
                .code(),
            4
        );
        assert_eq!(
            parse(b"not json", &questions()).unwrap_err().exit().code(),
            4
        );
        // An answer grevi did not ask for is ignored; a question with no answer is simply
        // absent, and reading it fails where it is read.
        let r = parse(
            br#"{"results":[{"dimensions":{"other":{"label":"x","scores":{"x":1.0}}}}]}"#,
            &questions(),
        )
        .unwrap();
        assert!(r.answers.is_empty());
        assert_eq!(r.noul("any").unwrap_err().exit().code(), 4);
    }

    #[test]
    fn error_bodies_carry_their_stable_code() {
        assert_eq!(
            error_message(r#"{"error":"at most 100 labels","code":"too_many_labels"}"#).unwrap(),
            "too_many_labels: at most 100 labels"
        );
        assert_eq!(
            error_message(r#"{"error":"nope"}"#).unwrap(),
            "nope".to_string()
        );
        assert_eq!(error_message("<html>502</html>"), None);
        assert_eq!(error_message(r#"{"detail":"x"}"#), None);
    }
}
