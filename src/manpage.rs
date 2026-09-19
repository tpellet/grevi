use regex::Regex;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

static OVERSTRIKE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r".\x08").unwrap());
static FLAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s{0,10}((?:-{1,2}[A-Za-z0-9][\w-]*)(?:,?\s+-{1,2}[A-Za-z0-9][\w-]*)*)((?:[ =]\[?<?[A-Za-z][\w.-]*>?\]?)*)\s{2,}(\S.*)$").unwrap()
});

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Flag {
    pub flag: String,
    pub takes_value: bool,
    pub desc: String,
}

/// Renders the man page with `man` only; hunch never runs the tool itself (not even `--help`)
/// to learn its options.
fn man_page(cmd: &str) -> Option<String> {
    let out = Command::new("man")
        .arg(cmd)
        .env("MANPAGER", "cat")
        .env("PAGER", "cat")
        .env("MANWIDTH", "200")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = OVERSTRIKE
        .replace_all(&String::from_utf8_lossy(&out.stdout), "")
        .into_owned();
    (!text.trim().is_empty()).then_some(text)
}

pub fn section(page: &str, name: &str) -> Option<String> {
    let mut lines = page.lines().skip_while(|l| l.trim() != name).skip(1);
    let mut body = Vec::new();
    for l in lines.by_ref() {
        let is_heading = !l.is_empty()
            && !l.starts_with(char::is_whitespace)
            && l.chars().all(|c| c.is_ascii_uppercase() || c == ' ');
        if is_heading {
            break;
        }
        body.push(l.trim());
    }
    let s = body
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!s.is_empty()).then_some(s)
}

pub fn description(cmd: &str, max_chars: usize) -> Option<String> {
    section(&man_page(cmd)?, "DESCRIPTION").map(|d| d.chars().take(max_chars).collect())
}

/// The OPTIONS section, preserving line structure for flag parsing.
pub fn options_text(cmd: &str) -> Option<String> {
    let page = man_page(cmd)?;
    let start = page
        .lines()
        .position(|l| matches!(l.trim(), "OPTIONS" | "DESCRIPTION"))?;
    Some(
        page.lines()
            .skip(start + 1)
            .take(600)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn parse_flags(text: &str) -> Vec<Flag> {
    let mut out: Vec<Flag> = Vec::new();
    for line in text.lines() {
        let Some(c) = FLAG.captures(line) else {
            continue;
        };
        let names: Vec<&str> = c[1]
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter(|s| s.starts_with('-'))
            .collect();
        let flag = names
            .iter()
            .find(|n| n.starts_with("--"))
            .or(names.first())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if flag.is_empty() || out.iter().any(|f| f.flag == flag) {
            continue;
        }
        out.push(Flag {
            flag,
            takes_value: !c[2].trim().is_empty(),
            desc: c[3].trim().to_string(),
        });
        if out.len() == 150 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_flags_with_values() {
        let help = "Usage: sips [options]\n  -z pixelsH pixelsW    Resample image at specified size\n  --help                 Show help\n  -v, --verbose          Verbose output\n";
        let f = parse_flags(help);
        assert_eq!(f[0].flag, "-z");
        assert!(f[0].takes_value);
        assert_eq!(f[2].flag, "--verbose");
        assert!(!f[2].takes_value);
    }
    #[test]
    fn extracts_description_section() {
        let page = "NAME\n     tar - manipulate tape archives\n\nDESCRIPTION\n     tar creates and manipulates streaming archive files.\n\nOPTIONS\n     -x  extract\n";
        assert_eq!(
            section(page, "DESCRIPTION").unwrap(),
            "tar creates and manipulates streaming archive files."
        );
    }
}
