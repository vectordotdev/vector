use std::collections::BTreeSet;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vector_config::{
    attributes::CustomAttribute,
    configurable_component,
    schema::{
        apply_base_metadata, generate_const_string_schema, generate_one_of_schema,
        generate_string_schema, generate_struct_schema, SchemaObject,
    },
    Configurable, Metadata, ToValue,
};
use vector_config_common::constants::LOGICAL_NAME;

/// Per metric set expiration options.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PerMetricSetExpiration {
    /// Metric name to apply this expiration to. Ignores metric name if not defined.
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub name: Option<MetricNameMatcherConfig>,
    /// Labels to apply this expiration. Ignores labels if not defined.
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub labels: Option<MetricLabelMatcherConfig>,
    /// The amount of time, in seconds, that internal metrics will persist after having not been
    /// updated before they expire and are removed.
    ///
    /// Set this to a value larger than your `internal_metrics` scrape interval (default 5 minutes)
    /// that metrics live long enough to be emitted and captured,
    pub expire_secs: f64,
}

/// Configuration for metric name matcher.
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "Matcher for metric name."))]
pub enum MetricNameMatcherConfig {
    /// Only considers exact name matches.
    Exact {
        /// The exact metric name.
        value: String,
    },
    /// Compares metric name to the provided pattern.
    Regex {
        /// Pattern to compare to.
        pattern: String,
    },
}

/// Configuration for metric labels matcher.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MetricLabelMatcherConfig {
    /// Checks that any of the provided matchers can be applied to given metric.
    Any {
        /// List of matchers to check.
        matchers: Vec<MetricLabelMatcherConfig>,
    },
    /// Checks that all of the provided matchers can be applied to given metric.
    All {
        /// List of matchers to check.
        matchers: Vec<MetricLabelMatcherConfig>,
    },
    /// Looks for an exact match of one label key value pair.
    Exact {
        /// Metric key to look for.
        key: String,
        /// The exact metric label value.
        value: String,
    },
    /// Compares label value with given key to the provided pattern.
    Regex {
        /// Metric key to look for.
        key: String,
        /// Pattern to compare metric label value to.
        value_pattern: String,
    },
}

impl ToValue for MetricLabelMatcherConfig {
    fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("Could not convert label matcher settings to JSON")
    }
}

fn generate_type_string_schema(
    logical_name: &str,
    title: Option<&'static str>,
    description: &'static str,
) -> SchemaObject {
    let mut const_schema = generate_const_string_schema(logical_name.to_lowercase());
    let mut const_metadata = Metadata::with_description(description);
    if let Some(title) = title {
        const_metadata.set_title(title);
    }
    const_metadata.add_custom_attribute(CustomAttribute::kv(LOGICAL_NAME, logical_name));
    apply_base_metadata(&mut const_schema, const_metadata);
    const_schema
}

// NOTE: Custom implementation of configurable to avoid stack overflow when running `generate_schema`
// due to cycle in schema (MetricLabelMatcherConfig can contain multiple MetricLabelMatcherConfig
// instances)
impl Configurable for MetricLabelMatcherConfig {
    fn generate_schema(
        _gen: &std::cell::RefCell<vector_config::schema::SchemaGenerator>,
    ) -> Result<vector_config::schema::SchemaObject, vector_config::GenerateError>
    where
        Self: Sized,
    {
        let string_metadata = Self::metadata();

        let any_subschema = generate_type_string_schema(
            "Any",
            Some("Any label match"),
            "Checks that any of the provided matchers can be applied to given metric.",
        );
        let all_subschema = generate_type_string_schema(
            "All",
            Some("All label match"),
            "Checks that all of the provided matchers can be applied to given metric.",
        );
        let exact_subschema = generate_type_string_schema(
            "Exact",
            Some("Exact label match"),
            "Looks for an exact match of one label key value pair.",
        );
        let regex_subschema = generate_type_string_schema(
            "Regex",
            Some("Regex label match"),
            "Compares label value with given key to the provided pattern.",
        );

        let mut type_subschema = generate_one_of_schema(&[
            any_subschema,
            all_subschema,
            exact_subschema,
            regex_subschema,
        ]);
        apply_base_metadata(&mut type_subschema, string_metadata);

        let mut required = BTreeSet::new();
        required.insert("type".to_string());

        let mut properties = IndexMap::new();
        properties.insert("type".to_string(), type_subschema.clone());
        properties.insert("key".to_string(), generate_string_schema());
        properties.insert("value".to_string(), generate_string_schema());
        properties.insert("value_pattern".to_string(), generate_string_schema());
        properties.insert("matchers".to_string(), generate_string_schema());

        let mut full_subschema = generate_struct_schema(properties, required, None);
        let mut full_metadata =
            Metadata::with_description("Configuration for metric labels matcher.");
        full_metadata.add_custom_attribute(CustomAttribute::flag("docs::hidden"));
        apply_base_metadata(&mut full_subschema, full_metadata);

        Ok(full_subschema)
    }

    fn referenceable_name() -> Option<&'static str>
    where
        Self: Sized,
    {
        Some(std::any::type_name::<Self>())
    }

    fn metadata() -> vector_config::Metadata
    where
        Self: Sized,
    {
        let mut metadata = Metadata::default();
        metadata.set_description("Configuration for metric labels matcher.");
        metadata.add_custom_attribute(CustomAttribute::kv(
            "docs::enum_tag_description",
            "Matcher for metric labels.",
        ));
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tagging", "internal"));
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tag_field", "type"));
        metadata
    }
}

/// Tests to confirm complex examples configuration
#[cfg(test)]
mod tests {
    use vrl::prelude::indoc;

    use super::*;

    #[test]
    fn just_expiration_config() {
        // This configuration should maybe be treated as invalid - because it turns into a global
        // configuration, matching every metric
        let config = serde_yaml::from_str::<PerMetricSetExpiration>(indoc! {r"
            expire_secs: 10.0
            "})
        .unwrap();

        assert!(config.name.is_none());
        assert!(config.labels.is_none());
        assert_eq!(10.0, config.expire_secs);
    }

    #[test]
    fn simple_name_config() {
        let config = serde_yaml::from_str::<PerMetricSetExpiration>(indoc! {r#"
            name:
                type: "exact"
                value: "test_metric"
            expire_secs: 1.0
            "#})
        .unwrap();

        if let Some(MetricNameMatcherConfig::Exact { value }) = config.name {
            assert_eq!("test_metric", value);
        } else {
            panic!("Expected exact name matcher");
        }
        assert!(config.labels.is_none());
        assert_eq!(1.0, config.expire_secs);
    }

    #[test]
    fn simple_labels_config() {
        let config = serde_yaml::from_str::<PerMetricSetExpiration>(indoc! {r#"
            labels:
                type: "exact"
                key: "test_metric_label"
                value: "test_value"
            expire_secs: 1.0
            "#})
        .unwrap();

        if let Some(MetricLabelMatcherConfig::Exact { key, value }) = config.labels {
            assert_eq!("test_metric_label", key);
            assert_eq!("test_value", value);
        } else {
            panic!("Expected exact label matcher");
        }
        assert!(config.name.is_none());
        assert_eq!(1.0, config.expire_secs);
    }

    #[test]
    fn complex_config() {
        let config = serde_yaml::from_str::<PerMetricSetExpiration>(indoc! {r#"
            name:
                type: "regex"
                pattern: "test_metric.*"
            labels:
                type: "all"
                matchers:
                    - type: "any"
                      matchers:
                          - type: "exact"
                            key: "component_kind"
                            value: "sink"
                          - type: "exact"
                            key: "component_kind"
                            value: "source"
                    - type: "regex"
                      key: "component_type"
                      value_pattern: "aws_.*"
                    - type: "any"
                      matchers:
                          - type: "exact"
                            key: "region"
                            value: "some_aws_region_name"
                          - type: "regex"
                            key: "endpoint"
                            value_pattern: "test.com.*"
            expire_secs: 1.0
            "#})
        .unwrap();

        if let Some(MetricNameMatcherConfig::Regex { ref pattern }) = config.name {
            assert_eq!("test_metric.*", pattern);
        } else {
            panic!("Expected regex name matcher");
        }

        let Some(MetricLabelMatcherConfig::All {
            matchers: all_matchers,
        }) = config.labels
        else {
            panic!("Expected all label matcher");
        };
        assert_eq!(3, all_matchers.len());

        let MetricLabelMatcherConfig::Any {
            matchers: first_any_matchers,
        } = &all_matchers[0]
        else {
            panic!("Expected first label matcher to be any matcher");
        };
        let MetricLabelMatcherConfig::Exact { key, value } = &first_any_matchers[0] else {
            panic!("Expected first any matcher to be exact matcher");
        };
        assert_eq!("component_kind", key);
        assert_eq!("sink", value);
        let MetricLabelMatcherConfig::Exact { key, value } = &first_any_matchers[1] else {
            panic!("Expected second any matcher to be exact matcher");
        };
        assert_eq!("component_kind", key);
        assert_eq!("source", value);

        let MetricLabelMatcherConfig::Regex { key, value_pattern } = &all_matchers[1] else {
            panic!("Expected second label matcher to be regex matcher");
        };
        assert_eq!("component_type", key);
        assert_eq!("aws_.*", value_pattern);

        let MetricLabelMatcherConfig::Any {
            matchers: second_any_matchers,
        } = &all_matchers[2]
        else {
            panic!("Expected third label matcher to be any matcher");
        };
        let MetricLabelMatcherConfig::Exact { key, value } = &second_any_matchers[0] else {
            panic!("Expected first any matcher to be exact matcher");
        };
        assert_eq!("region", key);
        assert_eq!("some_aws_region_name", value);
        let MetricLabelMatcherConfig::Regex { key, value_pattern } = &second_any_matchers[1] else {
            panic!("Expected second any matcher to be exact matcher");
        };
        assert_eq!("endpoint", key);
        assert_eq!("test.com.*", value_pattern);

        assert_eq!(1.0, config.expire_secs);
    }
}
