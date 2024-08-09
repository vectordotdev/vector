use crate::sinks::prelude::*;

// sub level implementations
mod logs;
mod metrics;

fn default_dbname() -> String {
    greptimedb_ingester::DEFAULT_SCHEMA_NAME.to_string()
}

fn default_dbname_template() -> Template {
    Template::try_from(default_dbname()).unwrap()
}

#[derive(Clone, Copy, Debug, Default)]
struct GreptimeDBDefaultBatchSettings;

impl SinkBatchSettings for GreptimeDBDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}
