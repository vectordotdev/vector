//! Deterministic input records shared by the scenarios.

use std::{fs::File, io::Write};

pub(super) fn records(prefix: &str, count: usize) -> Vec<String> {
    // Each record is large enough for the default 1024-byte fingerprint, including
    // scenarios that create a file containing a single record.
    (0..count)
        .map(|i| format!("{prefix}-{i:04} {}", ".".repeat(1024)))
        .collect()
}

pub(super) fn lines(records: &[String]) -> String {
    format!("{}\n", records.join("\n"))
}

pub(super) fn append_lines(file: &mut File, records: &[String]) -> vector::Result<()> {
    file.write_all(lines(records).as_bytes())?;
    file.sync_all()?;
    Ok(())
}
