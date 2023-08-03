use vector_core::config::{log_schema, LogNamespace};

pub mod optional;

pub(crate) fn get_message_field(log_namespace: LogNamespace) -> String {
    match log_namespace {
        LogNamespace::Vector => ".".to_string(),
        LogNamespace::Legacy => log_schema()
            .message_key()
            .expect("global log_schema.message_key to be valid path")
            .clone()
            .to_string(),
    }
}
