use std::time::Duration;

use metrics::Key;
use regex::Regex;

use crate::config::metrics_expiration::{
    MetricLabelMatcher, MetricLabelMatcherConfig, MetricNameMatcherConfig, PerMetricSetExpiration,
};

use super::recency::KeyMatcher;

pub(super) struct MetricKeyMatcher {
    name: Option<MetricNameMatcher>,
    labels: Option<LabelsMatcher>,
}

impl KeyMatcher<Key> for MetricKeyMatcher {
    fn matches(&self, key: &Key) -> bool {
        let name_match = self.name.as_ref().is_none_or(|m| m.matches(key));
        let labels_match = self.labels.as_ref().is_none_or(|l| l.matches(key));
        name_match && labels_match
    }
}

impl TryFrom<PerMetricSetExpiration> for MetricKeyMatcher {
    type Error = super::Error;

    fn try_from(value: PerMetricSetExpiration) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.name.map(TryInto::try_into).transpose()?,
            labels: value.labels.map(TryInto::try_into).transpose()?,
        })
    }
}

impl TryFrom<PerMetricSetExpiration> for (MetricKeyMatcher, Duration) {
    type Error = super::Error;

    fn try_from(value: PerMetricSetExpiration) -> Result<Self, Self::Error> {
        if value.expire_secs <= 0.0 {
            return Err(super::Error::TimeoutMustBePositive {
                timeout: value.expire_secs,
            });
        }
        let duration = Duration::from_secs_f64(value.expire_secs);
        Ok((value.try_into()?, duration))
    }
}

enum MetricNameMatcher {
    Exact(String),
    Regex(Regex),
}

impl KeyMatcher<Key> for MetricNameMatcher {
    fn matches(&self, key: &Key) -> bool {
        match self {
            MetricNameMatcher::Exact(name) => key.name() == name,
            MetricNameMatcher::Regex(regex) => regex.is_match(key.name()),
        }
    }
}

impl TryFrom<MetricNameMatcherConfig> for MetricNameMatcher {
    type Error = super::Error;

    fn try_from(value: MetricNameMatcherConfig) -> Result<Self, Self::Error> {
        Ok(match value {
            MetricNameMatcherConfig::Exact { value } => MetricNameMatcher::Exact(value),
            MetricNameMatcherConfig::Regex { pattern } => MetricNameMatcher::Regex(
                Regex::new(&pattern).map_err(|_| super::Error::InvalidRegexPattern { pattern })?,
            ),
        })
    }
}

enum LabelsMatcher {
    Any(Vec<LabelsMatcher>),
    All(Vec<LabelsMatcher>),
    Exact(String, String),
    Regex(String, Regex),
}

impl KeyMatcher<Key> for LabelsMatcher {
    fn matches(&self, key: &Key) -> bool {
        match self {
            LabelsMatcher::Any(vec) => vec.iter().any(|m| m.matches(key)),
            LabelsMatcher::All(vec) => vec.iter().all(|m| m.matches(key)),
            LabelsMatcher::Exact(label_key, label_value) => key
                .labels()
                .any(|l| l.key() == label_key && l.value() == label_value),
            LabelsMatcher::Regex(label_key, regex) => key
                .labels()
                .any(|l| l.key() == label_key && regex.is_match(l.value())),
        }
    }
}

impl TryFrom<MetricLabelMatcher> for LabelsMatcher {
    type Error = super::Error;

    fn try_from(value: MetricLabelMatcher) -> Result<Self, Self::Error> {
        Ok(match value {
            MetricLabelMatcher::Exact { key, value } => Self::Exact(key, value),
            MetricLabelMatcher::Regex { key, value_pattern } => Self::Regex(
                key,
                Regex::new(&value_pattern).map_err(|_| super::Error::InvalidRegexPattern {
                    pattern: value_pattern,
                })?,
            ),
        })
    }
}

impl TryFrom<MetricLabelMatcherConfig> for LabelsMatcher {
    type Error = super::Error;

    fn try_from(value: MetricLabelMatcherConfig) -> Result<Self, Self::Error> {
        Ok(match value {
            MetricLabelMatcherConfig::Any { matchers } => Self::Any(
                matchers
                    .into_iter()
                    .map(TryInto::<LabelsMatcher>::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            MetricLabelMatcherConfig::All { matchers } => Self::All(
                matchers
                    .into_iter()
                    .map(TryInto::<LabelsMatcher>::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use metrics::Label;
    use vrl::prelude::indoc;

    use super::*;

    const EMPTY: MetricKeyMatcher = MetricKeyMatcher {
        name: None,
        labels: None,
    };

    #[test]
    fn empty_matcher_should_match_all() {
        assert!(EMPTY.matches(&Key::from_name("test_name")));
        assert!(EMPTY.matches(&Key::from_parts(
            "another",
            [Label::new("test_key", "test_value")].iter()
        )));
    }

    #[test]
    fn name_matcher_should_ignore_labels() {
        let matcher = MetricKeyMatcher {
            name: Some(MetricNameMatcher::Exact("test_metric".to_string())),
            labels: None,
        };

        assert!(matcher.matches(&Key::from_name("test_metric")));
        assert!(matcher.matches(&Key::from_parts(
            "test_metric",
            [Label::new("test_key", "test_value")].iter()
        )));
        assert!(!matcher.matches(&Key::from_name("different_name")));
        assert!(!matcher.matches(&Key::from_parts(
            "different_name",
            [Label::new("test_key", "test_value")].iter()
        )));
    }

    #[test]
    fn exact_name_matcher_should_check_name() {
        let matcher = MetricKeyMatcher {
            name: Some(MetricNameMatcher::Exact("test_metric".to_string())),
            labels: None,
        };

        assert!(matcher.matches(&Key::from_name("test_metric")));
        assert!(!matcher.matches(&Key::from_name("different_name")));
        assert!(!matcher.matches(&Key::from_name("_test_metric")));
        assert!(!matcher.matches(&Key::from_name("test_metric123")));
    }

    #[test]
    fn regex_name_matcher_should_try_matching_the_name() {
        let matcher = MetricKeyMatcher {
            name: Some(MetricNameMatcher::Regex(
                Regex::new(r".*test_?metric.*").unwrap(),
            )),
            labels: None,
        };

        assert!(matcher.matches(&Key::from_name("test_metric")));
        assert!(!matcher.matches(&Key::from_name("different_name")));
        assert!(matcher.matches(&Key::from_name("_test_metric")));
        assert!(matcher.matches(&Key::from_name("test_metric123")));
        assert!(matcher.matches(&Key::from_name("__testmetric123")));
    }

    #[test]
    fn exact_label_matcher_should_look_for_exact_label_match() {
        let matcher = MetricKeyMatcher {
            name: None,
            labels: Some(LabelsMatcher::Exact(
                "test_key".to_string(),
                "test_value".to_string(),
            )),
        };

        assert!(!matcher.matches(&Key::from_name("test_metric")));
        assert!(matcher.matches(&Key::from_parts(
            "test_metric",
            [Label::new("test_key", "test_value")].iter()
        )));
        assert!(!matcher.matches(&Key::from_name("different_name")));
        assert!(matcher.matches(&Key::from_parts(
            "different_name",
            [Label::new("test_key", "test_value")].iter()
        )));
    }

    #[test]
    fn regex_label_matcher_should_look_for_exact_label_match() {
        let matcher = MetricKeyMatcher {
            name: None,
            labels: Some(LabelsMatcher::Regex(
                "test_key".to_string(),
                Regex::new(r"metric_val.*").unwrap(),
            )),
        };

        assert!(!matcher.matches(&Key::from_name("test_metric")));
        assert!(matcher.matches(&Key::from_parts(
            "test_metric",
            [Label::new("test_key", "metric_value123")].iter()
        )));
        assert!(!matcher.matches(&Key::from_parts(
            "test_metric",
            [Label::new("test_key", "test_value123")].iter()
        )));
        assert!(matcher.matches(&Key::from_parts(
            "different_name",
            [Label::new("test_key", "metric_val0")].iter()
        )));
    }

    #[test]
    fn any_label_matcher_should_look_for_at_least_one_match() {
        let matcher = MetricKeyMatcher {
            name: None,
            labels: Some(LabelsMatcher::Any(vec![
                LabelsMatcher::Regex("test_key".to_string(), Regex::new(r"metric_val.*").unwrap()),
                LabelsMatcher::Exact("test_key".to_string(), "test_value".to_string()),
            ])),
        };

        assert!(!matcher.matches(&Key::from_name("test_metric")));
        assert!(matcher.matches(&Key::from_parts(
            "test_metric",
            [Label::new("test_key", "metric_value123")].iter()
        )));
        assert!(matcher.matches(&Key::from_parts(
            "test_metric",
            [Label::new("test_key", "test_value")].iter()
        )));
        assert!(matcher.matches(&Key::from_parts(
            "different_name",
            [Label::new("test_key", "metric_val0")].iter()
        )));
        assert!(!matcher.matches(&Key::from_parts(
            "different_name",
            [Label::new("test_key", "different_value")].iter()
        )));
    }

    #[test]
    fn all_label_matcher_should_expect_all_matches() {
        let matcher = MetricKeyMatcher {
            name: None,
            labels: Some(LabelsMatcher::All(vec![
                LabelsMatcher::Regex("key_one".to_string(), Regex::new(r"metric_val.*").unwrap()),
                LabelsMatcher::Exact("key_two".to_string(), "test_value".to_string()),
            ])),
        };

        assert!(!matcher.matches(&Key::from_name("test_metric")));
        assert!(!matcher.matches(&Key::from_parts(
            "test_metric",
            [Label::new("key_one", "metric_value123")].iter()
        )));
        assert!(!matcher.matches(&Key::from_parts(
            "test_metric",
            [Label::new("key_two", "test_value")].iter()
        )));
        assert!(matcher.matches(&Key::from_parts(
            "different_name",
            [
                Label::new("key_one", "metric_value_1234"),
                Label::new("key_two", "test_value")
            ]
            .iter()
        )));
    }

    #[test]
    fn matcher_with_both_name_and_label_should_expect_both_to_match() {
        let matcher = MetricKeyMatcher {
            name: Some(MetricNameMatcher::Exact("test_metric".to_string())),
            labels: Some(LabelsMatcher::Exact(
                "test_key".to_string(),
                "test_value".to_string(),
            )),
        };

        assert!(!matcher.matches(&Key::from_name("test_metric")));
        assert!(!matcher.matches(&Key::from_name("different_name")));
        assert!(!matcher.matches(&Key::from_parts(
            "different_name",
            [Label::new("test_key", "test_value")].iter()
        )));
        assert!(matcher.matches(&Key::from_parts(
            "test_metric",
            [Label::new("test_key", "test_value")].iter()
        )));
    }

    #[test]
    fn complex_matcher_rules() {
        let matcher = MetricKeyMatcher {
            name: Some(MetricNameMatcher::Regex(Regex::new(r"custom_.*").unwrap())),
            labels: Some(LabelsMatcher::All(vec![
                // Let's match just sink metrics
                LabelsMatcher::Exact("component_kind".to_string(), "sink".to_string()),
                // And only AWS components
                LabelsMatcher::Regex("component_type".to_string(), Regex::new(r"aws_.*").unwrap()),
                // And some more rules
                LabelsMatcher::Any(vec![
                    LabelsMatcher::Exact("region".to_string(), "some_aws_region_name".to_string()),
                    LabelsMatcher::Regex(
                        "endpoint".to_string(),
                        Regex::new(r"test.com.*").unwrap(),
                    ),
                ]),
            ])),
        };

        assert!(!matcher.matches(&Key::from_name("test_metric")));
        assert!(!matcher.matches(&Key::from_name("custom_metric_a")));
        assert!(!matcher.matches(&Key::from_parts(
            "custom_metric_with_missing_component_type",
            [Label::new("component_kind", "sink")].iter()
        )));
        assert!(!matcher.matches(&Key::from_parts(
            "custom_metric_with_missing_extra_labels",
            [
                Label::new("component_kind", "sink"),
                Label::new("component_type", "aws_cloudwatch_metrics")
            ]
            .iter()
        )));
        assert!(!matcher.matches(&Key::from_parts(
            "custom_metric_with_wrong_region",
            [
                Label::new("component_kind", "sink"),
                Label::new("component_type", "aws_cloudwatch_metrics"),
                Label::new("region", "some_other_region")
            ]
            .iter()
        )));
        assert!(!matcher.matches(&Key::from_parts(
            "custom_metric_with_wrong_region_and_endpoint",
            [
                Label::new("component_kind", "sink"),
                Label::new("component_type", "aws_cloudwatch_metrics"),
                Label::new("region", "some_other_region"),
                Label::new("endpoint", "wrong_endpoint.com/metrics")
            ]
            .iter()
        )));
        assert!(matcher.matches(&Key::from_parts(
            "custom_metric_with_wrong_endpoint_but_correct_region",
            [
                Label::new("component_kind", "sink"),
                Label::new("component_type", "aws_cloudwatch_metrics"),
                Label::new("region", "some_aws_region_name"),
                Label::new("endpoint", "wrong_endpoint.com/metrics")
            ]
            .iter()
        )));
        assert!(matcher.matches(&Key::from_parts(
            "custom_metric_with_wrong_region_but_correct_endpoint",
            [
                Label::new("component_kind", "sink"),
                Label::new("component_type", "aws_cloudwatch_metrics"),
                Label::new("region", "some_other_region"),
                Label::new("endpoint", "test.com/metrics")
            ]
            .iter()
        )));
        assert!(!matcher.matches(&Key::from_parts(
            "custom_metric_with_wrong_component_kind",
            [
                Label::new("component_kind", "source"),
                Label::new("component_type", "aws_cloudwatch_metrics"),
                Label::new("region", "some_other_region"),
                Label::new("endpoint", "test.com/metrics")
            ]
            .iter()
        )));
    }

    #[test]
    fn parse_simple_config_into_matcher() {
        let config = serde_yaml::from_str::<PerMetricSetExpiration>(indoc! {r#"
            name:
                type: "exact"
                value: "test_metric"
            labels:
                type: "all"
                matchers:
                    - type: "exact"
                      key: "component_kind"
                      value: "sink"
                    - type: "regex"
                      key: "component_type"
                      value_pattern: "aws_.*"
            expire_secs: 1.0
            "#})
        .unwrap();

        let matcher: MetricKeyMatcher = config.try_into().unwrap();

        if let Some(MetricNameMatcher::Exact(value)) = matcher.name {
            assert_eq!("test_metric", value);
        } else {
            panic!("Expected exact name matcher");
        }

        let Some(LabelsMatcher::All(all_matchers)) = matcher.labels else {
            panic!("Expected main label matcher to be an all matcher");
        };

        assert_eq!(2, all_matchers.len());
        if let LabelsMatcher::Exact(key, value) = &all_matchers[0] {
            assert_eq!("component_kind", key);
            assert_eq!("sink", value);
        } else {
            panic!("Expected first label matcher to be an exact matcher");
        }
        if let LabelsMatcher::Regex(key, regex) = &all_matchers[1] {
            assert_eq!("component_type", key);
            assert_eq!("aws_.*", regex.as_str());
        } else {
            panic!("Expected second label matcher to be a regex matcher");
        }
    }
}
