#[cfg(feature = "transforms-dedupe")]
pub mod config;

#[cfg(feature = "transforms-impl-dedupe")]
pub mod transform;

#[cfg(feature = "transforms-impl-dedupe")]
pub mod common {
    use std::num::NonZeroUsize;

    use vector_lib::{configurable::configurable_component, lookup::lookup_v2::ConfigTargetPath};

    use crate::config::log_schema;

    /// Caching configuration for deduplication.
    #[configurable_component]
    #[derive(Clone, Debug)]
    #[serde(deny_unknown_fields)]
    pub struct CacheConfig {
        /// Number of events to cache and use for comparing incoming events to previously seen events.
        pub num_events: NonZeroUsize,
    }

    pub fn default_cache_config() -> CacheConfig {
        CacheConfig {
            num_events: NonZeroUsize::new(5000).expect("static non-zero number"),
        }
    }

    /// Options to control what fields to match against.
    ///
    /// When no field matching configuration is specified, events are matched using the `timestamp`,
    /// `host`, and `message` fields from an event. The specific field names used are those set in
    /// the global [`log schema`][global_log_schema] configuration.
    ///
    /// [global_log_schema]: https://vector.dev/docs/reference/configuration/global-options/#log_schema
    // TODO: This enum renders correctly in terms of providing equivalent Cue output when using the
    // machine-generated stuff vs the previously-hand-written Cue... but what it _doesn't_ have in the
    // machine-generated output is any sort of blurb that these "fields" (`match` and `ignore`) are
    // actually mutually exclusive.
    //
    // We know that to be the case when we're generating the output from the configuration schema, so we
    // need to emit something in that output to indicate as much, and further, actually use it on the
    // Cue side to add some sort of boilerplate about them being mutually exclusive, etc.
    #[configurable_component]
    #[derive(Clone, Debug)]
    #[serde(deny_unknown_fields)]
    pub enum FieldMatchConfig {
        /// Matches events using only the specified fields.
        #[serde(rename = "match")]
        MatchFields(
            #[configurable(metadata(
                docs::examples = "field1",
                docs::examples = "parent.child_field"
            ))]
            Vec<ConfigTargetPath>,
        ),

        /// Matches events using all fields except for the ignored ones.
        #[serde(rename = "ignore")]
        IgnoreFields(
            #[configurable(metadata(
                docs::examples = "field1",
                docs::examples = "parent.child_field",
                docs::examples = "host",
                docs::examples = "hostname"
            ))]
            Vec<ConfigTargetPath>,
        ),
    }

    pub fn fill_default_fields_match(maybe_fields: Option<&FieldMatchConfig>) -> FieldMatchConfig {
        // We provide a default value on `fields`, based on `default_match_fields`, in order to
        // drive the configuration schema and documentation. Since we're getting the values from the
        // configured log schema, though, the default field values shown in the configuration
        // schema/documentation may not be the same as an actual user's Vector configuration.
        match maybe_fields {
            Some(FieldMatchConfig::MatchFields(x)) => FieldMatchConfig::MatchFields(x.clone()),
            Some(FieldMatchConfig::IgnoreFields(y)) => FieldMatchConfig::IgnoreFields(y.clone()),
            None => FieldMatchConfig::MatchFields(default_match_fields()),
        }
    }

    // TODO: Add support to the `configurable(metadata(..))` helper attribute for passing an expression
    // that will provide the value for the metadata attribute's value, as well as letting all metadata
    // attributes have whatever value they want, so long as it can be serialized by `serde_json`.
    //
    // Once we have that, we could curry these default values (and others) via a metadata attribute
    // instead of via `serde(default = "...")` to allow for displaying default values in the
    // configuration schema _without_ actually changing how a field is populated during deserialization.
    //
    // See the comment in `fill_default_fields_match` for more information on why this is required.
    //
    // TODO: These values are used even for events with the new "Vector" log namespace.
    //   These aren't great defaults in that case, but hard-coding isn't much better since the
    //   structure can vary significantly. This should probably either become a required field
    //   in the future, or maybe the "semantic meaning" can be utilized here.
    fn default_match_fields() -> Vec<ConfigTargetPath> {
        let mut fields = Vec::new();
        if let Some(message_key) = log_schema().message_key_target_path() {
            fields.push(ConfigTargetPath(message_key.clone()));
        }
        if let Some(host_key) = log_schema().host_key_target_path() {
            fields.push(ConfigTargetPath(host_key.clone()));
        }
        if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
            fields.push(ConfigTargetPath(timestamp_key.clone()));
        }
        fields
    }
}
