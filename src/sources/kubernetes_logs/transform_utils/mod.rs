use vector_lib::config::{LogNamespace, log_schema};
use vrl::{owned_value_path, path::OwnedTargetPath};

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
