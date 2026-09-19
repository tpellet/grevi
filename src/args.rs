use crate::exit::HunchError;
use crate::inventory::Tool;
use crate::jev::client::Client;
use crate::jev::{Question, Questions};
use crate::manpage::Flag;
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Floors above the user's threshold for composing argv, a higher-stakes step than routing: a
/// flag needs its Noul at `max(threshold, FLAG_FLOOR)`, the file its Choice probability at
/// `max(threshold, FILE_FLOOR)`. Raising `-t` tightens both; lowering it never loosens them.
const FLAG_FLOOR: f64 = 0.7;
const FILE_FLOOR: f64 = 0.6;

static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]+)"|'([^']+)'"#).unwrap());
static TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[~./\w:-]*[\d./~][\w./~:-]*").unwrap());

pub fn value_candidates(request: &str) -> Vec<String> {
    let mut v: Vec<String> = QUOTED
        .captures_iter(request)
        .filter_map(|c| c.get(1).or(c.get(2)).map(|m| m.as_str().to_string()))
        .collect();
    let unquoted = QUOTED.replace_all(request, " ");
    for m in TOKEN.find_iter(&unquoted) {
        let t = m.as_str().trim_end_matches([',', '.', ':']);
        if !t.is_empty() && !v.iter().any(|x| x == t) {
            v.push(t.to_string());
        }
    }
    v.truncate(40);
    v
}

pub struct Proposal {
    pub argv: Vec<String>,
    pub flags: Vec<(String, f64)>,
    pub file: Option<(String, f64)>,
    pub placeholders: usize,
}

fn opts(ids: &[String], none: &str) -> BTreeMap<String, Option<String>> {
    let mut m: BTreeMap<String, Option<String>> = ids.iter().map(|i| (i.clone(), None)).collect();
    m.insert("NONE".into(), Some(none.into()));
    m
}

pub async fn propose(
    client: &Client,
    request: &str,
    tool: &Tool,
    flags: &[Flag],
    cwd: &[String],
    threshold: f64,
) -> Result<Proposal, HunchError> {
    let values = value_candidates(request);
    let files: Vec<String> = cwd.iter().take(200).cloned().collect();
    let state = serde_json::json!({
        "request": request,
        "tool": format!("{}: {}", tool.name, tool.summary),
        "flags": flags.iter().map(|f| format!("{} — {}", f.flag, f.desc)).collect::<Vec<_>>(),
        "values": values.iter().enumerate().map(|(i, v)| format!("[V{i:02}] {v}")).collect::<Vec<_>>(),
        "files": files.iter().enumerate().map(|(i, f)| format!("[F{i:03}] {f}")).collect::<Vec<_>>(),
    });
    let value_ids: Vec<String> = (0..values.len()).map(|i| format!("V{i:02}")).collect();
    let file_ids: Vec<String> = (0..files.len()).map(|i| format!("F{i:03}")).collect();
    let mut qs = Questions::new();
    for (i, f) in flags.iter().enumerate() {
        qs.insert(
            format!("use{i:03}"),
            Question::noul_with(
                format!("Does `request` ask for the behaviour described in `flags[{i}]`?"),
                "the request asks for exactly the behaviour this option provides",
                "the request does not ask for this option's behaviour",
            ),
        );
        if f.takes_value && !values.is_empty() {
            qs.insert(format!("val{i:03}"), Question::choice(
                format!("Which entry in `values` is the value the request gives for the option in `flags[{i}]`? Choose NONE if the request gives no value for it."),
                opts(&value_ids, "the request gives no value for this option"),
            ));
        }
    }
    if !files.is_empty() {
        qs.insert("file".into(), Question::choice(
            "Which entry in `files` does `request` refer to as the input or target of the command? Choose NONE if it refers to no listed file.",
            opts(&file_ids, "the request refers to no listed file"),
        ));
    }
    let r = client.ask(&state, &qs).await?;
    let mut argv = vec![tool.name.clone()];
    let mut chosen = Vec::new();
    let mut placeholders = 0;
    for (i, f) in flags.iter().enumerate() {
        // A missing answer is a protocol error, not "low confidence".
        let p = r.noul(&format!("use{i:03}"))?;
        if p < threshold.max(FLAG_FLOOR) {
            continue;
        }
        argv.push(f.flag.clone());
        chosen.push((f.flag.clone(), p));
        if f.takes_value {
            let pick = r
                .answers
                .get(&format!("val{i:03}"))
                .and_then(|a| a.choice.clone())
                .unwrap_or_else(|| "NONE".into());
            match pick
                .strip_prefix('V')
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| values.get(n))
            {
                Some(v) => argv.push(v.clone()),
                None => {
                    argv.push("<VALUE>".into());
                    placeholders += 1;
                }
            }
        }
    }
    let mut file = None;
    if let Some(a) = r.answers.get("file") {
        if let (Some(c), Some(probs)) = (&a.choice, &a.probabilities) {
            let p = probs.get(c).copied().unwrap_or(0.0);
            if let Some(f) = c
                .strip_prefix('F')
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| files.get(n))
            {
                if p >= threshold.max(FILE_FLOOR) {
                    argv.push(f.clone());
                    file = Some((f.clone(), p));
                }
            }
        }
    }
    Ok(Proposal {
        argv,
        flags: chosen,
        file,
        placeholders,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn value_candidates_copy_tokens_from_request() {
        let v =
            value_candidates("resize photo.jpg to 800 px tall, name it \"small copy\" in ~/out");
        assert!(v.contains(&"photo.jpg".to_string()));
        assert!(v.contains(&"800".to_string()));
        assert!(v.contains(&"small copy".to_string()));
        assert!(v.contains(&"~/out".to_string()));
    }
}
