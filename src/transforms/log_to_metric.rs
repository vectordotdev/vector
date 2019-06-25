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
pub struct CounterConfig {
    field: Atom,
    #[serde(skip)]
    sanitized_name: Atom,
    name: Option<Atom>,
    #[serde(default = "default_increment_by_value")]
    increment_by_value: bool,
    labels: IndexMap<Atom, String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct GaugeConfig {
    field: Atom,
    #[serde(skip)]
    sanitized_name: Atom,
    name: Option<Atom>,
    labels: IndexMap<Atom, String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MetricConfig {
    Counter(CounterConfig),
    Gauge(GaugeConfig),
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
        Ok(Box::new(LogToMetric::new(self)))
    }

    fn input_type(&self) -> crate::topology::config::DataType {
        crate::topology::config::DataType::Log
    }

    fn output_type(&self) -> crate::topology::config::DataType {
        crate::topology::config::DataType::Metric
    }
}

impl LogToMetric {
    pub fn new(config: &LogToMetricConfig) -> Self {
        let mut config = config.clone();

        for metric in config.metrics.iter_mut() {
            match metric {
                MetricConfig::Counter(ref mut counter) => {
                    let name = match &counter.name {
                        Some(s) => s.to_string(),
                        None => format!("{}_total", counter.field.to_string()),
                    };
                    counter.sanitized_name = Atom::from(name);
                }
                MetricConfig::Gauge(ref mut gauge) => {
                    let name = match &gauge.name {
                        Some(s) => s.to_string(),
                        None => gauge.field.to_string(),
                    };
                    gauge.sanitized_name = Atom::from(name);
                }
            }
        }

        LogToMetric { config }
    }
}

impl Transform for LogToMetric {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let event = event.into_log();

        for metric in self.config.metrics.iter() {
            match metric {
                MetricConfig::Counter(counter) => {
                    if let Some(val) = event.get(&counter.field) {
                        if counter.increment_by_value {
                            if let Ok(val) = val.to_string_lossy().parse::<f32>() {
                                return Some(Event::Metric(Metric::Counter {
                                    name: counter.sanitized_name.to_string(),
                                    val: val as u32,
                                    sampling: None,
                                }));
                            } else {
                                trace!("failed to parse counter value");
                                return None;
                            }
                        } else {
                            return Some(Event::Metric(Metric::Counter {
                                name: counter.sanitized_name.to_string(),
                                val: 1,
                                sampling: None,
                            }));
                        };
                    }
                }
                MetricConfig::Gauge(gauge) => {
                    if let Some(val) = event.get(&gauge.field) {
                        if let Ok(val) = val.to_string_lossy().parse() {
                            return Some(Event::Metric(Metric::Gauge {
                                name: gauge.sanitized_name.to_string(),
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

        let mut transform = LogToMetric::new(&config);

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

        let mut transform = LogToMetric::new(&config);

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

        let mut transform = LogToMetric::new(&config);

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
            .insert_explicit("amount".into(), "33.95".into());

        let mut transform = LogToMetric::new(&config);

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

        let mut transform = LogToMetric::new(&config);

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

        let mut transform = LogToMetric::new(&config);
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

        let mut transform = LogToMetric::new(&config);
        assert!(transform.transform(log).is_none());
    }
}
