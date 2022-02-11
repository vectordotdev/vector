use async_graphql::Enum;

#[derive(Enum, Copy, Clone, PartialEq, Eq)]
/// Encoding format for the event
pub enum EventEncodingType {
    Json,
    Yaml,
    Logfmt,
}
