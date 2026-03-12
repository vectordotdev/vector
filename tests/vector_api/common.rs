//! Common test utilities and pipeline configurations

use indoc::formatdoc;

/// Creates a single demo_logs source with blackhole sink
pub fn single_source_config(source_name: &str, interval_secs: f64, count: Option<u32>) -> String {
    let count_line = count
        .map(|c| format!("    count: {}\n", c))
        .unwrap_or_default();

    formatdoc! {"
        sources:
          {source_name}:
            type: demo_logs
            format: json
            interval: {interval_secs}
        {count_line}
        sinks:
          blackhole:
            type: blackhole
            inputs: ['{source_name}']
    "}
}

/// Creates two demo_logs sources with shared blackhole sink
pub fn dual_source_config(
    source1: &str,
    source2: &str,
    interval_secs: f64,
    count: Option<u32>,
) -> String {
    let count_line = count
        .map(|c| format!("    count: {}\n", c))
        .unwrap_or_default();

    formatdoc! {"
        sources:
          {source1}:
            type: demo_logs
            format: json
            interval: {interval_secs}
        {count_line}
          {source2}:
            type: demo_logs
            format: json
            interval: {interval_secs}
        {count_line}
        sinks:
          blackhole:
            type: blackhole
            inputs: ['{source1}', '{source2}']
    "}
}
