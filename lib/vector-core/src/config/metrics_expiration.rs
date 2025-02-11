use vector_config::configurable_component;

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
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "Matcher for metric labels."))]
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
