//! classifier.dev as a second backend.
//!
//! classifier.dev is a free, keyless front end to the same Jev model jevify asks through
//! TypeSafe (its upstream error codes are literally `typesafe_<status>`, and a live call
//! answers `model: "jev-1.13.0"`). So this module is a wire translation, not a second
//! design: jevify's questions go out as *dimensions* on one item and the answers come back
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
use crate::exit::JevifyError;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Dimensions per request (`maxProperties: 20` on `ClassifyRequest.dimensions`).
pub const MAX_DIMENSIONS: usize = 20;
/// Labels per dimension (`maxItems: 100`).
pub const MAX_LABELS: usize = 100;
/// UTF-16 code units per input (the service applies JavaScript `string.length`).
pub const MAX_INPUT_CHARS: usize = 32_000;
/// UTF-16 code units per dimension's `instructions`.
const MAX_INSTRUCTIONS_CHARS: usize = 4_000;
/// The two labels a Noul becomes when its criteria cannot be labels themselves.
const YES: &str = "yes";
const NO: &str = "no";
/// Label length limit in UTF-16 code units.
const MAX_LABEL_CHARS: usize = 200;

/// The two labels a Noul is asked as, `true` first. A Noul's criteria are two sentences
/// ("this hunk implements the described work" / "this hunk is about something else"), and
/// classifier.dev answers a semantic label better than a bare yes/no — its own guidance is
/// that "'urgent bug' classifies better than 'p0'". Criteria that cannot be labels (absent,
/// equal, blank, or past the 200-character limit) fall back to `yes`/`no`, with the sentences
/// in the instructions instead.
fn noul_labels(criteria: Option<&super::NoulCriteria>) -> (String, String) {
    let usable = |s: &str| {
        !s.trim().is_empty() && s.encode_utf16().count() <= MAX_LABEL_CHARS && !s.contains('\n')
    };
    match criteria {
        Some(c) if usable(&c.yes) && usable(&c.no) && c.yes != c.no => {
            (c.yes.clone(), c.no.clone())
        }
        _ => (YES.to_string(), NO.to_string()),
    }
}

/// The state as the one text classifier.dev classifies. A string state is its own text (that
/// is what `is` sends); anything else is compact JSON. Preflight rejects oversized items.
pub fn item_text(state: &serde_json::Value) -> String {
    match state {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// One question as a dimension definition: the labels it may answer with, and the instructions
/// that carry everything Jev would read from `criteria`.
fn dimension(q: &Question) -> Result<serde_json::Value, JevifyError> {
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
    if !(2..=MAX_LABELS).contains(&labels.len())
        || labels
            .iter()
            .any(|label| label.trim().is_empty() || label.encode_utf16().count() > MAX_LABEL_CHARS)
    {
        return Err(JevifyError::InputTooLarge(
            "classifier requires 2..=100 nonempty labels of at most 200 characters".into(),
        ));
    }
    if instructions.encode_utf16().count() > MAX_INSTRUCTIONS_CHARS {
        return Err(JevifyError::InputTooLarge(
            "classifier instructions exceed 4000 characters".into(),
        ));
    }
    Ok(serde_json::json!({
        "labels": labels,
        "instructions": instructions,
    }))
}

/// The body of one `POST /v1/classify`: items sharing one dimension per question.
pub fn request_body(
    items: &[String],
    questions: &Questions,
) -> Result<serde_json::Value, JevifyError> {
    if items.is_empty() || items.iter().any(|item| item.trim().is_empty()) {
        return Err(JevifyError::Usage(
            "classifier item must not be empty".into(),
        ));
    }
    if items
        .iter()
        .any(|item| item.encode_utf16().count() > MAX_INPUT_CHARS)
    {
        return Err(JevifyError::InputTooLarge(
            "classifier item exceeds 32000 characters".into(),
        ));
    }
    if questions.is_empty() || questions.len() > MAX_DIMENSIONS {
        return Err(JevifyError::InputTooLarge(
            "classifier requires 1..=20 dimensions".into(),
        ));
    }
    if items.len() > 1_000 / questions.len() {
        return Err(JevifyError::InputTooLarge(
            "classifier requires at most 1000 decisions".into(),
        ));
    }
    if questions
        .keys()
        .any(|id| id.trim().is_empty() || id.encode_utf16().count() > 64)
    {
        return Err(JevifyError::Usage(
            "classifier dimension names require 1..=64 characters and non-whitespace text".into(),
        ));
    }
    let dimensions: serde_json::Map<String, serde_json::Value> = questions
        .iter()
        .map(|(id, q)| dimension(q).map(|d| (id.clone(), d)))
        .collect::<Result<_, _>>()?;
    // The service's readDimensions checks JSON.stringify(dimensions).length: compact JSON
    // including escaping and wrapper fields, measured in JavaScript UTF-16 code units.
    let definitions =
        serde_json::to_string(&dimensions).map_err(|e| JevifyError::Protocol(e.to_string()))?;
    if definitions.encode_utf16().count() > 16_000 {
        return Err(JevifyError::InputTooLarge(
            "classifier dimension definitions exceed 16000 UTF-16 code units".into(),
        ));
    }
    Ok(serde_json::json!({ "items": items, "dimensions": dimensions }))
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

/// Lenient for the same reason as [`Response`]: a field jevify does not read must not fail a
/// command. `results` stays required.
#[derive(Deserialize, Debug)]
struct ClassifyResponse {
    #[serde(default)]
    model: String,
    results: Vec<Item>,
}

/// The classifier.dev answer for one chunk, read back into jevify's [`Response`].
///
/// A Noul's probability is P(yes) from `scores`; with no scores (the API documents them as
/// nullable) the verdict's own `confidence` stands in, mirrored for a `no`. A Choice keeps its
/// scores as probabilities; with no scores there is no distribution to rank items by, which is
/// a protocol error (exit 4), never a silently invented one.
pub fn parse_each(
    body: &[u8],
    questions: &Questions,
    count: usize,
) -> Result<Vec<Response>, JevifyError> {
    let parsed: ClassifyResponse =
        serde_json::from_slice(body).map_err(|e| JevifyError::Protocol(e.to_string()))?;
    if parsed.results.len() != count {
        return Err(JevifyError::Protocol(format!(
            "classify returned {} results; expected {count}",
            parsed.results.len()
        )));
    }
    parsed
        .results
        .into_iter()
        .map(|item| parse_item(item, &parsed.model, questions))
        .collect()
}

fn parse_item(
    item: Item,
    fallback_model: &str,
    questions: &Questions,
) -> Result<Response, JevifyError> {
    let mut answers = BTreeMap::new();
    let mut model = String::new();
    for (id, q) in questions {
        let r = item
            .dimensions
            .get(id)
            .ok_or_else(|| JevifyError::Protocol(format!("missing dimension `{id}`")))?;
        if r.confidence.is_some_and(|p| !super::valid_probability(p)) {
            return Err(JevifyError::Protocol(format!(
                "invalid confidence for `{id}`"
            )));
        }
        let dimension_model = super::normalize_model(r.model.as_deref().unwrap_or(fallback_model));
        super::join_models(&mut model, dimension_model);
        let answer = match q {
            Question::Noul { criteria, .. } => {
                let (yes, no) = noul_labels(criteria.as_ref());
                if !r
                    .label
                    .as_ref()
                    .is_some_and(|label| label == &yes || label == &no)
                    || r.scores.as_ref().is_some_and(|scores| {
                        scores.len() != 2
                            || [&yes, &no].iter().any(|label| {
                                !scores
                                    .get(*label)
                                    .copied()
                                    .is_some_and(super::valid_probability)
                            })
                    })
                {
                    return Err(JevifyError::Protocol(format!(
                        "invalid noul labels or scores for `{id}`"
                    )));
                }
                let p = match (&r.scores, &r.label, r.confidence) {
                    (Some(s), _, _) => s
                        .get(&yes)
                        .copied()
                        .ok_or_else(|| JevifyError::Protocol(format!("no yes score for `{id}`")))?,
                    (None, Some(label), Some(c)) if *label == yes => c,
                    (None, Some(label), Some(c)) if *label == no => 1.0 - c,
                    _ => {
                        return Err(JevifyError::Protocol(format!(
                            "no probability for `{id}`; classifier.dev returned neither scores nor a confident verdict"
                        )));
                    }
                };
                Answer {
                    noul: Some(p),
                    ..Answer::default()
                }
            }
            Question::Choice { .. } => Answer {
                choice: r.label.clone(),
                probabilities: Some(r.scores.clone().ok_or_else(|| {
                    JevifyError::Protocol(format!("no scores for choice `{id}`"))
                })?),
                confidence: r.confidence,
                ..Answer::default()
            },
        };
        answers.insert(id.clone(), answer);
    }
    let response = Response {
        model,
        answers,
        // classifier.dev counts classifications, not tokens: it is free, and `meta.cost_usd`
        // stays 0 because nothing was spent.
        usage: Usage::default(),
    };
    response.validate(questions)?;
    Ok(response)
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

    #[test]
    fn missing_and_blank_dimension_models_keep_unknown_after_fallback() {
        let qs: Questions = [("a".into(), Question::noul("a"))].into();
        for model in [None, Some(""), Some("  ")] {
            let mut dimension = serde_json::json!({"label":"yes", "confidence":0.8});
            if let Some(model) = model {
                dimension["model"] = model.into();
            }
            let mut body = serde_json::json!({"results":[{"dimensions":{"a":dimension}}]});
            let response = parse_each(&serde_json::to_vec(&body).unwrap(), &qs, 1).unwrap();
            assert_eq!(response[0].model, "unknown");
            body["model"] = "jev-fallback".into();
            let response = parse_each(&serde_json::to_vec(&body).unwrap(), &qs, 1).unwrap();
            assert_eq!(
                response[0].model,
                if model.is_none() {
                    "jev-fallback"
                } else {
                    "unknown"
                }
            );
            body.as_object_mut().unwrap().remove("model");
            body["results"][0]["dimensions"]["b"] =
                serde_json::json!({"label":"yes", "confidence":0.8, "model":"jev-fake"});
            let mut mixed = qs.clone();
            mixed.insert("b".into(), Question::noul("b"));
            let response = parse_each(&serde_json::to_vec(&body).unwrap(), &mixed, 1).unwrap();
            assert_eq!(response[0].model, "unknown, jev-fake");
            assert!(!response[0].all_jev());
        }
    }

    #[test]
    fn batch_result_count_must_match_and_models_keep_first_seen_order() {
        let qs: Questions = [
            ("a".into(), Question::noul("a")),
            ("b".into(), Question::noul("b")),
            ("c".into(), Question::noul("c")),
        ]
        .into();
        let item = serde_json::json!({"dimensions": {
            "a": {"label":"yes","confidence":0.8,"model":"other-model"},
            "b": {"label":"yes","confidence":0.8,"model":"jev-fake"},
            "c": {"label":"yes","confidence":0.8,"model":"other-model"}
        }});
        for count in [0, 1, 2, 3] {
            let bytes = serde_json::to_vec(&serde_json::json!({"model":"unused-fallback", "results":vec![item.clone(); count]})).unwrap();
            let result = parse_each(&bytes, &qs, 2);
            if count == 2 {
                let responses = result.unwrap();
                assert_eq!(responses.len(), 2);
                for response in responses {
                    assert_eq!(response.model, "other-model, jev-fake");
                    assert!(!response.all_jev());
                }
            } else {
                assert_eq!(result.unwrap_err().kind(), "api_protocol");
            }
        }
    }

    #[test]
    fn request_decision_limit_and_empty_items_are_checked() {
        let qs: Questions = [("q".into(), Question::noul("q"))].into();
        assert!(request_body(&[], &qs).is_err());
        assert!(request_body(&[" ".into()], &qs).is_err());
        assert!(request_body(&vec!["x".into(); 1000], &qs).is_ok());
        assert!(request_body(&vec!["x".into(); 1001], &qs).is_err());
        let qs: Questions = [
            ("a".into(), Question::noul("a")),
            ("b".into(), Question::noul("b")),
        ]
        .into();
        assert!(request_body(&vec!["x".into(); 500], &qs).is_ok());
        assert!(request_body(&vec!["x".into(); 501], &qs).is_err());
    }

    fn parse(body: &[u8], questions: &Questions) -> Result<Response, JevifyError> {
        Ok(parse_each(body, questions, 1)?.remove(0))
    }

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
        let body = request_body(
            &[item_text(&serde_json::json!({ "items": ["a", "b"] }))],
            &questions(),
        )
        .unwrap();
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
        let body = request_body(&["text".into()], &qs).unwrap();
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
    fn instructions_and_input_are_rejected_over_the_api_limits() {
        let mut qs = Questions::new();
        qs.insert("q".into(), Question::noul("x".repeat(9_000)));
        assert_eq!(
            request_body(&["y".repeat(MAX_INPUT_CHARS * 2)], &qs,)
                .unwrap_err()
                .exit()
                .code(),
            6
        );
        assert_eq!(
            request_body(&["small".into()], &qs)
                .unwrap_err()
                .exit()
                .code(),
            6
        );
    }

    #[test]
    fn constructed_fields_accept_exact_limits_and_reject_one_over() {
        let choice = |n, instructions: String| {
            Question::choice(
                instructions,
                (0..n).map(|i| (format!("L{i}"), None)).collect(),
            )
        };
        let mut qs: Questions = [("q".into(), choice(100, "é".repeat(4000)))].into();
        assert!(request_body(&["é".repeat(32_000)], &qs).is_ok());
        assert!(request_body(&["é".repeat(32_001)], &qs).is_err());
        qs.insert("q".into(), choice(101, "x".into()));
        assert!(request_body(&["x".into()], &qs).is_err());
        qs.insert("q".into(), choice(2, "x".repeat(4001)));
        assert!(request_body(&["x".into()], &qs).is_err());
        for n in [200, 201] {
            qs.insert(
                "q".into(),
                Question::choice("x", [("é".repeat(n), None), ("NONE".into(), None)].into()),
            );
            assert_eq!(request_body(&["x".into()], &qs).is_ok(), n == 200);
        }
        for n in [20, 21] {
            qs = (0..n)
                .map(|i| (format!("q{i}"), choice(2, "x".into())))
                .collect();
            assert_eq!(request_body(&["x".into()], &qs).is_ok(), n == 20);
        }
        for n in [64, 65] {
            qs = [("é".repeat(n), choice(2, "x".into()))].into();
            assert_eq!(request_body(&["x".into()], &qs).is_ok(), n == 64);
        }
        assert!(request_body(&["".into()], &questions()).is_err());
    }

    #[test]
    fn aggregate_definition_limit_counts_serialized_utf16_including_escapes() {
        let mut qs: Questions = (0..4)
            .map(|i| {
                (
                    format!("q{i}"),
                    Question::noul_with("😀".repeat(1800), "matches", "different"),
                )
            })
            .collect();
        let body = request_body(&["x".into()], &qs).unwrap();
        let remaining = 16_000 - body["dimensions"].to_string().encode_utf16().count();
        // Newlines count as two characters in compact JSON because they are escaped.
        for i in 0..remaining / 2 {
            let Question::Noul { instructions, .. } = qs.get_mut(&format!("q{}", i % 4)).unwrap()
            else {
                unreachable!()
            };
            instructions.push('\n');
        }
        if !remaining.is_multiple_of(2) {
            let Question::Noul { instructions, .. } = qs.get_mut("q0").unwrap() else {
                unreachable!()
            };
            instructions.push('x');
        }
        let body = request_body(&["x".into()], &qs).unwrap();
        assert_eq!(
            body["dimensions"].to_string().encode_utf16().count(),
            16_000
        );
        let Question::Noul { instructions, .. } = qs.get_mut("q0").unwrap() else {
            unreachable!()
        };
        instructions.push('x');
        assert_eq!(
            request_body(&["x".into()], &qs).unwrap_err().exit().code(),
            6
        );
    }

    #[test]
    fn classifier_limits_count_astral_characters_as_two_utf16_units() {
        for n in [16_000, 16_001] {
            assert_eq!(
                request_body(&["😀".repeat(n)], &questions()).is_ok(),
                n == 16_000
            );
        }
        for n in [2_000, 2_001] {
            let qs = [(
                "q".into(),
                Question::noul_with("😀".repeat(n), "matches", "different"),
            )]
            .into();
            assert_eq!(request_body(&["x".into()], &qs).is_ok(), n == 2_000);
        }
        for n in [100, 101] {
            let qs = [(
                "q".into(),
                Question::choice("x", [("😀".repeat(n), None), ("NONE".into(), None)].into()),
            )]
            .into();
            assert_eq!(request_body(&["x".into()], &qs).is_ok(), n == 100);
        }
        for n in [32, 33] {
            let qs = [("😀".repeat(n), Question::noul("x"))].into();
            assert_eq!(request_body(&["x".into()], &qs).is_ok(), n == 32);
        }
    }

    #[test]
    fn malformed_noul_labels_scores_and_confidences_fail() {
        let qs = [("q".into(), Question::noul("is it?"))].into();
        for value in [
            serde_json::json!({"label":"wrong","confidence":0.8,"scores":null}),
            serde_json::json!({"label":"yes","confidence":1.1,"scores":null}),
            serde_json::json!({"label":"yes","scores":{"yes":1.1,"no":0.0}}),
            serde_json::json!({"label":"yes","scores":{"yes":0.8}}),
            serde_json::json!({"scores":{"yes":0.8,"no":0.2}}),
        ] {
            let body =
                serde_json::to_vec(&serde_json::json!({"results":[{"dimensions":{"q":value}}]}))
                    .unwrap();
            assert_eq!(parse(&body, &qs).unwrap_err().exit().code(), 4);
        }
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
        // Unknown answers are harmless, but a missing requested dimension fails immediately.
        let r = parse(
            br#"{"results":[{"dimensions":{"other":{"label":"x","scores":{"x":1.0}}}}]}"#,
            &questions(),
        );
        assert_eq!(r.unwrap_err().exit().code(), 4);
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
