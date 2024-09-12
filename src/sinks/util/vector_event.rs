// Structs used for our vector event logs
use regex::Regex;

// Struct for vector send events (sending, uploaded)
#[derive(Clone, Debug)]
pub struct VectorSendEventMetadata {
    pub bytes: usize,
    pub events_len: usize,
    pub blob: String,
    pub container: String,
    pub topic: String,
}

impl VectorSendEventMetadata {
    pub fn emit_upload_event(&self) {
        info!(
            message = "Uploaded events.",
            bytes = self.bytes,
            events_len = self.events_len,
            blob = self.blob,
            container = self.container,
            topic = self.topic,
            // VECTOR_UPLOADED_MESSAGES_EVENT
            vector_event_type = 4
        );
    }

    pub fn emit_sending_event(&self) {
        info!(
            message = "Sending events.",
            bytes = self.bytes,
            events_len = self.events_len,
            blob = self.blob,
            container = self.container,
            topic = self.topic,
            // VECTOR_SENDING_MESSAGES_EVENT
            vector_event_type = 3
        );
    }
}

// Utility function for extracting the topic name from an archived log file path.
pub fn extract_topic_name(file_path: &str) -> String {
    // Topic: If the file being uploaded matches the archived-log filepattern we can extract
    // its topic from said pattern; otherwise propagate the empty-string.
    let topic_regex = Regex::new(r"archived-log\/log-sync-internal\/(?:structured-log\/)?([a-zA-Z-_]+)\/date").unwrap();
    let topic_capture = topic_regex.captures(file_path);
    if !topic_capture.is_none() {
        return topic_capture.unwrap()[1].to_string();
        
    }
    return "".to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_topic_name_success() {
        let file_path = "databricks-logs/archived-log/log-sync-internal/test-topic/date=2024-04-02/us-west-2/vector-aggregator-0/test.log";
        assert_eq!(extract_topic_name(file_path), "test-topic");
        let file_path_structured = "databricks-logs/archived-log/log-sync-internal/structured-log/test-topic/date=2024-04-02/us-west-2/vector-aggregator-0/test.log";
        assert_eq!(extract_topic_name(file_path_structured), "test-topic");
    }

    #[test]
    fn extract_topic_name_fail() {
        let file_path = r"no-topic";
        assert_eq!(extract_topic_name(file_path), "");
    }
}
