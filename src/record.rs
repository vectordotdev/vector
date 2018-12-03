#[derive(PartialEq, Debug, Clone)]
pub struct Record {
    pub(crate) line: String,
    pub(crate) timestamp: chrono::DateTime<chrono::Utc>,
}

impl Record {
    pub fn new_from_line(line: String) -> Self {
        Record {
            line,
            timestamp: chrono::Utc::now(),
        }
    }
}
