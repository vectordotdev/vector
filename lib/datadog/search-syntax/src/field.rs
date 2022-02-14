use super::grammar;

/// Default fields that represent the search path when a Datadog tag/facet is not provided.
static DEFAULT_FIELDS: &[&str] = &[
    "message",
    "custom.error.message",
    "custom.error.stack",
    "custom.title",
    "_default_",
];

/// Attributes that represent special fields in Datadog.
static RESERVED_ATTRIBUTES: &[&str] = &[
    "host",
    "source",
    "status",
    "service",
    "trace_id",
    "message",
    "timestamp",
    "tags",
];

/// Describes a field to search on.
#[derive(Clone, Hash, PartialEq, Eq)]
pub enum Field {
    /// Default field (when tag/facet isn't provided)
    Default(String),

    /// Reserved field that receives special treatment in Datadog.
    Reserved(String),

    /// A facet -- i.e. started with `@`, transformed to `custom.*`
    Facet(String),

    /// Tag type - i.e. search in the `tags` field.
    Tag(String),
}

impl Field {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Default(ref s) => s,
            Self::Reserved(ref s) => s,
            Self::Facet(ref s) => s,
            Self::Tag(ref s) => s,
        }
    }
}

/// Converts a field/facet name to the VRL equivalent. Datadog payloads have a `message` field
/// (which is used whenever the default field is encountered. Facets are hosted on .custom.*.
pub fn normalize_fields<T: AsRef<str>>(value: T) -> Vec<Field> {
    let value = value.as_ref();
    if value.eq(grammar::DEFAULT_FIELD) {
        return DEFAULT_FIELDS
            .iter()
            .map(|s| Field::Default((*s).to_owned()))
            .collect();
    }

    let field = match value.replace("@", "custom.") {
        v if value.starts_with('@') => Field::Facet(v),
        v if DEFAULT_FIELDS.contains(&v.as_ref()) => Field::Default(v),
        v if RESERVED_ATTRIBUTES.contains(&v.as_ref()) => Field::Reserved(v),
        v => Field::Tag(v),
    };

    vec![field]
}
