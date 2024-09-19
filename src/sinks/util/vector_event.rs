// Structs used for our vector event logs
use regex::Regex;

use std::collections::HashMap;
use std::env;
use once_cell::sync::OnceCell;
use serde_json;

use crate::event::Event;
use crate::event::LogEvent;

use crate::vector_lib::ByteSizeOf;

pub static EVENT_LOG_METADATA_FIELD: OnceCell<String> = OnceCell::new();
pub static EVENT_LOG_GRANULARITY_FIELDS: OnceCell<Vec<String>> = OnceCell::new();

// Where we can find the log metadata object
pub fn get_event_log_metadata_field() -> &'static String {
    // Initialize the static variable once, or return the value if it's already initialized/computed
    EVENT_LOG_METADATA_FIELD.get_or_init(|| {
            env::var("EVENT_LOG_METADATA_FIELD").unwrap_or_else(|_| "".to_string())
        }
    )
}

// Within the log metadata object itself, these are fields we care to parse for
pub fn get_event_log_granularity_fields() -> &'static Vec<String> {
    // Initialize the static variable once, or return the value if it's already initialized/computed
    EVENT_LOG_GRANULARITY_FIELDS.get_or_init(|| {
            let vec_string: String = env::var("EVENT_LOG_GRANULARITY_FIELDS").unwrap_or_else(|_| "".to_string());
            let keys: Vec<String> = serde_json::from_str(&vec_string).unwrap_or(Vec::new());
            keys
        }
    )
}

/*
* Struct to help track the count/size of events per unique combination of specified fields
*/
#[derive(Clone, Debug)]
pub struct MetadataValuesCount {
    pub value_map: HashMap<String, String>,
    pub count: usize,
    pub size: usize,
}

// Struct for vector send events (sending, uploaded)
#[derive(Clone, Debug)]
pub struct VectorEventLogSendMetadata {
    pub bytes: usize,
    pub events_len: usize,
    pub blob: String,
    pub container: String,
    // Count map here allows us to keep track of the count/size of events per combination of fields
    // Key is a string encoding those combinations for ease of update
    pub count_map: HashMap<String, MetadataValuesCount>,
}

impl VectorEventLogSendMetadata {
    pub fn emit_upload_event(&self) {
        // VECTOR_UPLOADED_MESSAGES_EVENT
        self.emit_count_map("Uploaded events.", 4)
    }

    pub fn emit_sending_event(&self) {
        // VECTOR_SENDING_MESSAGES_EVENT
        self.emit_count_map("Sending events.", 3)
    }

    fn emit_count_map(&self, message: &str, event_type: usize) {
        for (_, value) in &self.count_map {
            info!(
                message = message,
                keys = serde_json::to_string(&value.value_map).unwrap(),
                bytes = value.size,
                events_len = value.count,
                blob = self.blob,
                container = self.container,
                vector_event_type = event_type,
            );
        }
    }
}

// Function to get the events of a desired field and encode them in a key so we more easily keep
// a map tracking size / count per unique combination of field values
fn build_key(event: &LogEvent) -> String {
    let mut key_vals: Vec<String> = Vec::new();
    // Get the field that holds the metadata struct itself
    let field = get_event_log_metadata_field();
    for key_part in get_event_log_granularity_fields() {
        if let Ok(value) = event.parse_path_and_get_value(format!("{}.{}", field, key_part)) {
            if let Some(val) = value {
               key_vals.push(format!("{}={}", key_part, val.to_string()));
            }
        }
    }
    key_vals.join("/")
}

// Creates a map with the values of the desired fields (i.e. {plane: PLANE_CONTROL})
fn build_map(event: &LogEvent) -> HashMap<String, String> {
   let mut val_map = HashMap::new();
   let field = get_event_log_metadata_field();
   for key_part in get_event_log_granularity_fields() {
       if let Ok(value) = event.parse_path_and_get_value(format!("{}.{}", field, key_part)) {
           if let Some(val) = value {
              // Remove extra quotes from string
              val_map.insert(key_part.to_string(), val.to_string().replace("\"", ""));
           }
       }
   }
   val_map
}

/*
* On a list of events, iterate through them and track the counts per unique combination of
* specified fields
*
* The map here is String -> MetadataValuesCount
* where the String is an encoded key of the combination and values
* and MetadataValuesCount is a struct that holds the count, size, and a map of the values
*/
pub fn generate_count_map(events: &Vec<Event>) -> HashMap<String, MetadataValuesCount> {
    let mut count_map = HashMap::new();
    for event in events {
        // Check if it's a log event (see enum defined in lib/vector-core/src/event/mod.rs)
        if let Event::Log(log_event) = event {
            count_map.entry(build_key(log_event))
                .and_modify(
                    |x: &mut MetadataValuesCount| {
                        x.count += 1;
                        // For now, using pre-defined allocated bytes measure for size of event
                        // This may not be fully consistent with the real size of logs
                        // But having this a placeholder as consistent size measurement is tricky
                        x.size += log_event.size_of();
                    }
                )
                .or_insert(
                    MetadataValuesCount {
                        value_map: build_map(log_event),
                        count: 1,
                        size: 0,
                    }
                );
        }
    }
    count_map
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
