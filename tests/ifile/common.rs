//! Deterministic input records shared by the scenarios.

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
