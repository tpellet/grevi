use crate::exit::{Exit, JevifyError};

pub mod add;
pub mod agent;
pub mod fill;
pub mod filter;
pub mod is;
pub mod label;
pub mod pick;
pub mod run;
pub mod sort;
pub mod why;

/// The gate scores of a ranking: the two top Choice probabilities, P(NONE) and the Noul.
pub(crate) fn gate_of(ranking: &crate::tournament::Ranking) -> crate::output::Gate {
    crate::output::Gate {
        best: ranking.candidates.first().map(|c| c.p),
        next: ranking.candidates.get(1).map(|c| c.p),
        none: Some(ranking.none),
        any: Some(ranking.any),
    }
}

/// Result of a verb: exit code, machine data, and the exact human stdout text.
pub struct Outcome {
    pub exit: Exit,
    pub data: serde_json::Value,
    pub human: Vec<u8>,
    pub exec: Option<Exec>,
}

pub struct Exec {
    pub argv: Vec<std::ffi::OsString>,
    pub stdin_null: bool,
}

/// Asks on /dev/tty so it works when stdout is piped. `Ok(None)` means there is no TTY
/// (never act); `Ok(Some(false))` means the user declined. Shared by `run` and `add`.
pub fn confirm_tty(prompt: &str) -> Result<Option<bool>, JevifyError> {
    use std::io::{BufRead, Write};
    let Ok(tty) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
    else {
        return Ok(None);
    };
    let mut w = &tty;
    let _ = write!(w, "{prompt}");
    let _ = w.flush();
    let mut line = String::new();
    std::io::BufReader::new(&tty)
        .read_line(&mut line)
        .map_err(|e| JevifyError::Input(e.to_string()))?;
    Ok(Some(matches!(line.trim(), "y" | "Y" | "yes")))
}
