use vector_lib::config::{log_schema, LogNamespace};
use vrl::owned_value_path;
use vrl::path::OwnedTargetPath;

pub mod optional;

pub(crate) fn get_message_path(log_namespace: LogNamespace) -> OwnedTargetPath {
    match log_namespace {
        LogNamespace::Vector => OwnedTargetPath::event(owned_value_path!()),
        LogNamespace::Legacy => OwnedTargetPath::event(
            log_schema()
                .message_key()
                .expect("global log_schema.message_key to be valid path")
                .clone(),
        ),
    }
}
