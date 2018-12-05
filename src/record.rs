use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

#[derive(PartialEq, Debug, Clone)]
pub struct Record {
    pub(crate) line: String,
    pub(crate) timestamp: chrono::DateTime<chrono::Utc>,
    pub(crate) custom: HashMap<Atom, String>,
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
