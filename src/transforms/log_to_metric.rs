use super::Transform;
use crate::{event::metric::Metric, Event};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct LogToMetricConfig {
    pub metrics: Vec<MetricConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MetricConfig {
    Counter {
        field: Atom,
        name: Option<Atom>,
        #[serde(default = "default_increment_by_value")]
        increment_by_value: bool,
        labels: IndexMap<Atom, String>,
    },
    Gauge {
        field: Atom,
        name: Option<Atom>,
        labels: IndexMap<Atom, String>,
    },
}

fn default_increment_by_value() -> bool {
    false
}

pub struct LogToMetric {
    config: LogToMetricConfig,
}

#[typetag::serde(name = "log_to_metric")]
impl crate::topology::config::TransformConfig for LogToMetricConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(LogToMetric::new(self.clone())))
    }

    fn input_type(&self) -> crate::topology::config::DataType {
        crate::topology::config::DataType::Log
    }

    fn output_type(&self) -> crate::topology::config::DataType {
        crate::topology::config::DataType::Metric
    }
}

impl LogToMetric {
    pub fn new(config: LogToMetricConfig) -> Self {
        LogToMetric { config }
    }
}

impl Transform for LogToMetric {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let event = event.into_log();

        for metric in self.config.metrics.iter() {
            match metric {
                MetricConfig::Counter {
                    field,
                    name,
                    increment_by_value,
                    ..
                } => {
                    if let Some(val) = event.get(field) {
                        let name = match name {
                            Some(s) => s.to_string(),
                            None => format!("{}_total", field.to_string()),
                        };

                        if *increment_by_value {
                            if let Ok(val) = val.to_string_lossy().parse() {
                                return Some(Event::Metric(Metric::Counter {
                                    name,
                                    val,
                                    sampling: None,
                                }));
                            } else {
                                trace!("failed to parse counter value");
                                return None;
                            }
                        } else {
                            return Some(Event::Metric(Metric::Counter {
                                name,
                                val: 1,
                                sampling: None,
                            }));
                        };
                    }
                }
                MetricConfig::Gauge { field, name, .. } => {
                    if let Some(val) = event.get(field) {
                        let name = match name {
                            Some(s) => s.to_string(),
                            None => format!("{}", field.to_string()),
                        };

                        if let Ok(val) = val.to_string_lossy().parse() {
                            return Some(Event::Metric(Metric::Gauge {
                                name,
                                val,
                                direction: None,
                            }));
                        } else {
                            trace!("failed to parse gauge value");
                            return None;
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::{LogToMetric, LogToMetricConfig};
    use crate::{event::metric::Metric, transforms::Transform, Event};

    #[test]
    fn count_http_status_codes() {
        let config: LogToMetricConfig = toml::from_str(
            r##"
            [[metrics]]
            type = "counter"
            field = "status"
            labels = {status = "#{event.status}", host = "#{event.host}"}
            "##,
        )
        .unwrap();

        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("status".into(), "42".into());

        let mut transform = LogToMetric::new(config);

        let metric = transform.transform(log).unwrap();
        assert_eq!(
            metric.into_metric(),
            Metric::Counter {
                name: "status_total".into(),
                val: 1,
                sampling: None
            }
        );
    }

    #[test]
    fn count_exceptions() {
        let config: LogToMetricConfig = toml::from_str(
            r##"
            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "exception_total"
            labels = {host = "#{event.host}"}
            "##,
        )
        .unwrap();

        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("backtrace".into(), "message".into());

        let mut transform = LogToMetric::new(config);

        let metric = transform.transform(log).unwrap();
        assert_eq!(
            metric.into_metric(),
            Metric::Counter {
                name: "exception_total".into(),
                val: 1,
                sampling: None
            }
        );
    }

    #[test]
    fn count_exceptions_no_match() {
        let config: LogToMetricConfig = toml::from_str(
            r##"
            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "exception_total"
            labels = {host = "#{event.host}"}
            "##,
        )
        .unwrap();

        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("success".into(), "42".into());

        let mut transform = LogToMetric::new(config);

        let metric = transform.transform(log);
        assert!(metric.is_none());
    }

    #[test]
    fn sum_order_amounts() {
        let config: LogToMetricConfig = toml::from_str(
            r##"
            [[metrics]]
            type = "counter"
            field = "amount"
            name = "amount_total"
            increment_by_value = true
            labels = {host = "#{event.host}"}
            "##,
        )
        .unwrap();

        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("amount".into(), "33".into());

        let mut transform = LogToMetric::new(config);

        let metric = transform.transform(log).unwrap();
        assert_eq!(
            metric.into_metric(),
            Metric::Counter {
                name: "amount_total".into(),
                val: 33,
                sampling: None
            }
        );
    }

    #[test]
    fn memory_usage_guage() {
        let config: LogToMetricConfig = toml::from_str(
            r##"
            [[metrics]]
            type = "gauge"
            field = "memory_rss"
            name = "memory_rss_bytes"
            labels = {host = "#{event.host}"}
            "##,
        )
        .unwrap();

        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("memory_rss".into(), "123".into());

        let mut transform = LogToMetric::new(config);

        let metric = transform.transform(log).unwrap();
        assert_eq!(
            metric.into_metric(),
            Metric::Gauge {
                name: "memory_rss_bytes".into(),
                val: 123,
                direction: None,
            }
        );
    }

    #[test]
    fn parse_failure() {
        let config: LogToMetricConfig = toml::from_str(
            r##"
            [[metrics]]
            type = "counter"
            field = "status"
            name = "status_total"
            increment_by_value = true
            labels = {status = "#{event.status}", host = "#{event.host}"}
            "##,
        )
        .unwrap();

        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("status".into(), "not a number".into());

        let mut transform = LogToMetric::new(config);
        assert!(transform.transform(log).is_none());
    }

    #[test]
    fn missing_field() {
        let config: LogToMetricConfig = toml::from_str(
            r##"
            [[metrics]]
            type = "counter"
            field = "status"
            name = "status_total"
            labels = {status = "#{event.status}", host = "#{event.host}"}
            "##,
        )
        .unwrap();

        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("not foo".into(), "not a number".into());

        let mut transform = LogToMetric::new(config);
        assert!(transform.transform(log).is_none());
    }
}
