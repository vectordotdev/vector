use vector_core::config::log_schema;
use vector_core::schema;
use vrl::value::Kind;

/// Inspect the global log schema and create a schema requirement.
pub fn get_serializer_schema_requirement() -> schema::Requirement {
    if let Some(message_key) = log_schema().message_key() {
        schema::Requirement::empty().required_meaning(message_key.to_string(), Kind::any())
    } else {
        schema::Requirement::empty()
    }
}
