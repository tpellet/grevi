use regex::Regex;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

static OVERSTRIKE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r".\x08").unwrap());
const SYNOPSIS_MAX_CHARS: usize = 1000;

/// Renders the man page without running the tool itself, even with `--help`.
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
    if !out.status.success() {
        return None;
    }
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

/// The tool's synopsis, bounded to 1000 characters. Call from a blocking worker.
pub fn synopsis(cmd: &str) -> Option<String> {
    synopsis_from_page(man_page(cmd).as_deref())
}

fn synopsis_from_page(page: Option<&str>) -> Option<String> {
    section(page?, "SYNOPSIS").map(|s| s.chars().take(SYNOPSIS_MAX_CHARS).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_description_section() {
        let page = "NAME\n     tar - manipulate tape archives\n\nDESCRIPTION\n     tar creates and manipulates streaming archive files.\n\nOPTIONS\n     -x  extract\n";
        assert_eq!(
            section(page, "DESCRIPTION").unwrap(),
            "tar creates and manipulates streaming archive files."
        );
    }

    #[test]
    fn extracts_synopsis_section() {
        let page = "NAME\n    tool - inspect files\nSYNOPSIS\n    tool [-v]\n        file ...\n\nDESCRIPTION\n    Inspect files.\n";
        assert_eq!(
            synopsis_from_page(Some(page)).as_deref(),
            Some("tool [-v] file ...")
        );
    }

    #[test]
    fn clips_long_synopsis_at_a_character_boundary() {
        let text = "é".repeat(SYNOPSIS_MAX_CHARS + 1);
        let page = format!("SYNOPSIS\n    {text}\nDESCRIPTION\n    description\n");
        assert_eq!(
            synopsis_from_page(Some(&page)),
            Some("é".repeat(SYNOPSIS_MAX_CHARS))
        );
        let exact = format!("SYNOPSIS\n    {}", "x".repeat(SYNOPSIS_MAX_CHARS));
        assert_eq!(
            synopsis_from_page(Some(&exact)).unwrap().len(),
            SYNOPSIS_MAX_CHARS
        );
    }

    #[test]
    fn no_man_page_has_no_synopsis() {
        assert_eq!(synopsis_from_page(None), None);
    }

    #[test]
    fn missing_or_empty_synopsis_has_no_text() {
        for page in [
            "",
            "NAME\n    tool\nDESCRIPTION\n    inspect files\n",
            "SYNOPSIS\n\nDESCRIPTION\n    inspect files\n",
        ] {
            assert_eq!(synopsis_from_page(Some(page)), None);
        }
    }
}
