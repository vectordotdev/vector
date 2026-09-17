//! Deterministic input records shared by the scenarios.

pub(super) fn records(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{prefix}-{i:04}")).collect()
}

pub(super) fn lines(records: &[String]) -> String {
    format!("{}\n", records.join("\n"))
}
