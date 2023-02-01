use async_graphql::Enum;

#[derive(Enum, Copy, Clone, PartialEq, Eq)]
/// Encoding format for the event
pub(crate) enum EventEncodingType {
    Json,
    Yaml,
    Logfmt,
}
