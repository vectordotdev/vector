use std::collections::HashMap;
use std::env;
use once_cell::sync::OnceCell;
use serde_json;

use crate::event::Event;
use crate::event::LogEvent;

// Structs used for our vector event logs

pub static EVENT_LOG_METADATA_FIELD: OnceCell<String> = OnceCell::new();
pub static EVENT_LOG_METADATA_KEYS: OnceCell<Vec<String>> = OnceCell::new();

pub fn get_event_log_metadata_field() -> &'static String {
    // Initialize the static variable once, or return the value if it's already initialized/computed
    EVENT_LOG_METADATA_FIELD.get_or_init(|| {
            env::var("EVENT_LOG_METADATA_FIELD").unwrap_or_else(|_| "".to_string())
        }
    )
}

pub fn get_event_log_metadata_keys() -> &'static Vec<String> {
    // Initialize the static variable once, or return the value if it's already initialized/computed
    EVENT_LOG_METADATA_KEYS.get_or_init(|| {
            let vec_string: String = env::var("EVENT_LOG_METADATA_KEYS").unwrap_or_else(|_| "".to_string());
            let keys: Vec<String> = serde_json::from_str(&vec_string).unwrap_or(Vec::new());
            keys
        }
    )
}

#[derive(Clone, Debug)]
pub struct MetadataValuesCount {
    pub value_map: HashMap<String, String>,
    pub count: usize,
}

// impl PartialEq for MetadataValues {
//     fn eq(&self, other: &Self) -> bool {
//         self.key == other.key
//     }
// }
//
// impl Eq for MetadataValues {}
//
// impl std::hash::Hash for MetadataValues {
//     fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
//         self.key.hash(state);
//     }
// }
//
// impl MetadataValues {
//
// }

// Struct for vector send events (sending, uploaded)
#[derive(Clone, Debug)]
pub struct VectorEventLogSendMetadata {
    pub bytes: usize,
    pub events_len: usize,
    pub blob: String,
    pub container: String,
    pub count_map: HashMap<String, MetadataValuesCount>,
}

impl VectorEventLogSendMetadata {
    pub fn emit_upload_event(&self) {
        info!(
            message = "Uploaded events.",
            bytes = self.bytes,
            events_len = self.events_len,
            blob = self.blob,
            container = self.container,
            // VECTOR_UPLOADED_MESSAGES_EVENT
            vector_event_type = 4
        );
        // Also sending the count break down by specified keys
        self.emit_count_map("Uploaded granularity breakdown.", 4)
    }

    pub fn emit_sending_event(&self) {
        info!(
            message = "Sending events.",
            bytes = self.bytes,
            events_len = self.events_len,
            blob = self.blob,
            container = self.container,
            // VECTOR_SENDING_MESSAGES_EVENT
            vector_event_type = 3
        );
        // Also sending the count break down by specified keys
        self.emit_count_map("Sending granularity breakdown.", 3)
    }

    fn emit_count_map(&self, message: &str, event_type: usize) {
        for (_, value) in &self.count_map {
            info!(
                message = message,
                keys = serde_json::to_string(&value.value_map).unwrap(),
                count = value.count,
                blob = self.blob,
                container = self.container,
                vector_event_type = event_type,
            );
        }
    }
}

pub fn build_key(event: &LogEvent) -> String {
    let mut key_vals: Vec<String> = Vec::new();
    // Get the field that holds the metadata struct itself
    let field = get_event_log_metadata_field();
    for key_part in get_event_log_metadata_keys() {
        if let Ok(value) = event.parse_path_and_get_value(format!("{}.{}", field, key_part)) {
            if let Some(val) = value {
               // Remove extra quotes from string
               key_vals.push(format!("{}={}", key_part, val.to_string().replace("\"", "")));
            }
        }
    }
    key_vals.join("/")
}

pub fn build_map(event: &LogEvent) -> HashMap<String, String> {
   let mut val_map = HashMap::new();
   let field = get_event_log_metadata_field();
   for key_part in get_event_log_metadata_keys() {
       if let Ok(value) = event.parse_path_and_get_value(format!("{}.{}", field, key_part)) {
           if let Some(val) = value {
              // Remove extra quotes from string
              val_map.insert(key_part.to_string(), val.to_string().replace("\"", ""));
           }
       }
   }
   val_map
}

pub fn generate_count_map(events: &Vec<Event>) -> HashMap<String, MetadataValuesCount> {
    let mut count_map = HashMap::new();
    for event in events {
        // Check if it's a log event (see enum defined in lib/vector-core/src/event/mod.rs)
        if let Event::Log(log_event) = event {
            count_map.entry(build_key(log_event))
                .and_modify(|x: &mut MetadataValuesCount| x.count += 1)
                .or_insert(MetadataValuesCount { value_map: build_map(log_event), count: 1 });
        }
    }
    count_map
}
