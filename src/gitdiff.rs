#[derive(Debug, Clone)]
pub struct Hunk {
    pub header: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    /// `diff --git` line through the `+++` line, verbatim.
    pub header: String,
    pub hunks: Vec<Hunk>,
}

pub fn parse(diff: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            files.push(FileDiff {
                header: line.to_string(),
                hunks: vec![],
            });
        } else if let Some(f) = files.last_mut() {
            if line.starts_with("@@") {
                f.hunks.push(Hunk {
                    header: line.to_string(),
                    body: String::new(),
                });
            } else if let Some(h) = f.hunks.last_mut() {
                h.body.push_str(line);
            } else {
                f.header.push_str(line);
            }
        }
    }
    files
}

pub fn patch(files: &[FileDiff], keep: &dyn Fn(usize, usize) -> bool) -> String {
    let mut out = String::new();
    for (fi, f) in files.iter().enumerate() {
        let hunks: String = f
            .hunks
            .iter()
            .enumerate()
            .filter(|(hi, _)| keep(fi, *hi))
            .map(|(_, h)| format!("{}{}", h.header, h.body))
            .collect();
        if !hunks.is_empty() {
            out.push_str(&f.header);
            out.push_str(&hunks);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_and_patch_roundtrip_selected_hunks() {
        let diff = "diff --git a/a.txt b/a.txt\nindex 1..2 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-x\n+y\n z\n@@ -10,2 +10,2 @@\n-p\n+q\n r\n";
        let files = parse(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hunks.len(), 2);
        let p = patch(&files, &|_, h| h == 1);
        assert!(p.contains("@@ -10,2 +10,2 @@") && !p.contains("@@ -1,2 +1,2 @@"));
        assert!(p.starts_with("diff --git a/a.txt b/a.txt"));
    }
}
