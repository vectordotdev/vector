use std::fmt::Debug;

use chrono::{DateTime, Local, ParseError, TimeZone as _, Utc};
use chrono_tz::Tz;
use derivative::Derivative;
use vector_config::{
    schema::{
        apply_metadata, generate_const_string_schema, generate_one_of_schema,
        get_or_generate_schema,
    },
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError, Metadata,
};
use vector_config_common::attributes::CustomAttribute;

/// Timezone reference.
///
/// This can refer to any valid timezone as defined in the [TZ database][tzdb], or "local" which
/// refers to the system local timezone.
///
/// [tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
#[cfg_attr(
    feature = "serde",
    derive(::serde::Deserialize, ::serde::Serialize),
    serde(try_from = "String", into = "String")
)]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
pub enum TimeZone {
    /// System local timezone.
    #[derivative(Default)]
    Local,

    /// A named timezone.
    ///
    /// Must be a valid name in the [TZ database][tzdb].
    ///
    /// [tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
    Named(Tz),
}

/// This is a wrapper trait to allow `TimeZone` types to be passed generically.
impl TimeZone {
    /// Parse a date/time string into `DateTime<Utc>`.
    ///
    /// # Errors
    ///
    /// Returns parse errors from the underlying time parsing functions.
    pub fn datetime_from_str(&self, s: &str, format: &str) -> Result<DateTime<Utc>, ParseError> {
        match self {
            Self::Local => Local
                .datetime_from_str(s, format)
                .map(|dt| datetime_to_utc(&dt)),
            Self::Named(tz) => tz
                .datetime_from_str(s, format)
                .map(|dt| datetime_to_utc(&dt)),
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "" | "local" => Some(Self::Local),
            _ => s.parse::<Tz>().ok().map(Self::Named),
        }
    }
}

/// Convert a timestamp with a non-UTC time zone into UTC
pub(super) fn datetime_to_utc<TZ: chrono::TimeZone>(ts: &DateTime<TZ>) -> DateTime<Utc> {
    Utc.timestamp_opt(ts.timestamp(), ts.timestamp_subsec_nanos())
        .single()
        .expect("invalid timestamp")
}

impl From<TimeZone> for String {
    fn from(tz: TimeZone) -> Self {
        match tz {
            TimeZone::Local => "local".to_string(),
            TimeZone::Named(tz) => tz.name().to_string(),
        }
    }
}

impl TryFrom<String> for TimeZone {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match TimeZone::parse(&value) {
            Some(tz) => Ok(tz),
            None => Err("No such time zone".to_string()),
        }
    }
}

// TODO: Consider an approach for generating schema of "fixed string value, or remainder" structure
// used by this type.
impl Configurable for TimeZone {
    fn referenceable_name() -> Option<&'static str> {
        Some(std::any::type_name::<Self>())
    }

    fn metadata() -> vector_config::Metadata<Self> {
        let mut metadata = vector_config::Metadata::default();
        metadata.set_title("Timezone reference.");
        metadata.set_description(r#"This can refer to any valid timezone as defined in the [TZ database][tzdb], or "local" which refers to the system local timezone.

[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones"#);
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tagging", "untagged"));
        metadata.add_custom_attribute(CustomAttribute::kv("docs::examples", "local"));
        metadata.add_custom_attribute(CustomAttribute::kv("docs::examples", "America/New_York"));
        metadata.add_custom_attribute(CustomAttribute::kv("docs::examples", "EST5EDT"));
        metadata
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        let mut local_schema = generate_const_string_schema("local".to_string());
        let mut local_metadata = Metadata::<()>::with_description("System local timezone.");
        local_metadata.add_custom_attribute(CustomAttribute::kv("logical_name", "Local"));
        apply_metadata(&mut local_schema, local_metadata);

        let mut tz_metadata = Metadata::with_title("A named timezone.");
        tz_metadata.set_description(
            r#"Must be a valid name in the [TZ database][tzdb].

[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones"#,
        );
        tz_metadata.add_custom_attribute(CustomAttribute::kv("logical_name", "Named"));
        let tz_schema = get_or_generate_schema::<Tz>(gen, tz_metadata)?;

        Ok(generate_one_of_schema(&[local_schema, tz_schema]))
    }
}
