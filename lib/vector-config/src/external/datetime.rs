use crate::{
    schema::{
        apply_base_metadata, generate_const_string_schema, generate_one_of_schema,
        get_or_generate_schema, SchemaGenerator, SchemaObject,
    },
    Configurable, GenerateError, Metadata, ToValue,
};
use chrono_tz::Tz;
use serde_json::Value;
use std::cell::RefCell;
use vector_config_common::{attributes::CustomAttribute, constants};
use vrl::compiler::TimeZone;

// TODO: Consider an approach for generating schema of "fixed string value, or remainder" structure
// used by this type.
impl Configurable for TimeZone {
    fn referenceable_name() -> Option<&'static str> {
        Some(std::any::type_name::<Self>())
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_title("Timezone to use for any date specifiers in template strings.");
        metadata.set_description(r#"This can refer to any valid timezone as defined in the [TZ database][tzdb], or "local" which refers to the system local timezone. It will default to the [globally configured timezone](https://vector.dev/docs/reference/configuration/global-options/#timezone).

[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones"#);
        metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_ENUM_TAGGING,
            "untagged",
        ));
        metadata.add_custom_attribute(CustomAttribute::kv(constants::DOCS_META_EXAMPLES, "local"));
        metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_EXAMPLES,
            "America/New_York",
        ));
        metadata.add_custom_attribute(CustomAttribute::kv(
            constants::DOCS_META_EXAMPLES,
            "EST5EDT",
        ));
        metadata
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        let mut local_schema = generate_const_string_schema("local".to_string());
        let mut local_metadata = Metadata::with_description("System local timezone.");
        local_metadata.add_custom_attribute(CustomAttribute::kv("logical_name", "Local"));
        apply_base_metadata(&mut local_schema, local_metadata);

        let mut tz_metadata = Metadata::with_title("A named timezone.");
        tz_metadata.set_description(
            r#"Must be a valid name in the [TZ database][tzdb].

[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones"#,
        );
        tz_metadata.add_custom_attribute(CustomAttribute::kv("logical_name", "Named"));
        let tz_schema = get_or_generate_schema(&Tz::as_configurable_ref(), gen, Some(tz_metadata))?;

        Ok(generate_one_of_schema(&[local_schema, tz_schema]))
    }
}

impl ToValue for TimeZone {
    fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("Could not convert time zone to JSON")
    }
}
