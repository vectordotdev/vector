use std::collections::HashMap;
use std::sync::Arc;

#[derive(PartialEq, Debug, Clone)]
pub struct Record {
    pub(crate) line: String,
    pub(crate) timestamp: chrono::DateTime<chrono::Utc>,
    pub(crate) custom: HashMap<Arc<String>, String>,
}

impl Record {
    pub fn new_from_line(line: String) -> Self {
        Record {
            line,
            timestamp: chrono::Utc::now(),
            custom: HashMap::new(),
        }
    }
}
