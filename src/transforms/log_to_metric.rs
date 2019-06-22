use super::Transform;
use crate::{event::metric::Metric, Event};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct LogToMetricConfig {
    pub counters: Vec<CounterConfig>,
    pub metrics: Vec<MetricConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CounterConfig {
    field: Atom,
    parse_value: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MetricConfig {
    Counter {
        field: Atom,
        name: Atom,
        labels: IndexMap<Atom, String>,
        #[serde(default = "default_increment_by_value")]
        increment_by_value: bool,
    },
    Gauge {
        field: Atom,
        name: Atom,
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
                MetricConfig::Counter { .. } => {}
                MetricConfig::Gauge { field, name, .. } => {
                    if let Some(val) = event.get(field) {
                        if let Ok(val) = val.to_string_lossy().parse() {
                            return Some(Event::Metric(Metric::Gauge {
                                name: name.to_string(),
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

        for counter in self.config.counters.iter() {
            if let Some(val) = event.get(&counter.field) {
                if counter.parse_value {
                    if let Ok(val) = val.to_string_lossy().parse() {
                        return Some(Event::Metric(Metric::Counter {
                            name: counter.field.to_string(),
                            val,
                            sampling: None,
                        }));
                    } else {
                        trace!("failed to parse counter value");
                        return None;
                    }
                } else {
                    return Some(Event::Metric(Metric::Counter {
                        name: counter.field.to_string(),
                        val: 1,
                        sampling: None,
                    }));
                };
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::{LogToMetric, LogToMetricConfig};
    use crate::{event::metric::Metric, transforms::Transform, Event};

    fn config() -> LogToMetricConfig {
        toml::from_str(
            r##"
            counters = [{field = "foo", parse_value = true}, {field = "bar", parse_value = false}]

            [[metrics]]
            type = "counter"
            field = "status"
            name = "status_total"
            increment_by_value = false
            labels = {status = "#{event.status}", host = "#{event.host}"}
            "##,
        )
        .unwrap()
    }

    #[test]
    fn count_http_status_codes() {
        let config: LogToMetricConfig = toml::from_str(
            r##"
            counters = [{field = "foo", parse_value = true}, {field = "bar", parse_value = false}]

            [[metrics]]
            type = "counter"
            field = "status"
            name = "status_total"
            increment_by_value = false
            labels = {status = "#{event.status}", host = "#{event.host}"}
            "##,
        )
        .unwrap();

        let mut log = Event::from("i am a log");
        log.as_mut_log().insert_explicit("foo".into(), "42".into());

        let mut transform = LogToMetric::new(config);

        let metric = transform.transform(log).unwrap();
        assert_eq!(
            metric.into_metric(),
            Metric::Counter {
                name: "foo".into(),
                val: 42,
                sampling: None
            }
        );
    }

    #[test]
    fn counter_with_parsing() {
        let mut log = Event::from("i am a log");
        log.as_mut_log().insert_explicit("foo".into(), "42".into());

        let mut transform = LogToMetric::new(config());

        let metric = transform.transform(log).unwrap();
        assert_eq!(
            metric.into_metric(),
            Metric::Counter {
                name: "foo".into(),
                val: 42,
                sampling: None
            }
        );
    }

    #[test]
    fn counter_without_parsing() {
        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("bar".into(), "nineteen".into());

        let mut transform = LogToMetric::new(config());

        let metric = transform.transform(log).unwrap();
        assert_eq!(
            metric.into_metric(),
            Metric::Counter {
                name: "bar".into(),
                val: 1,
                sampling: None
            }
        );
    }

    #[test]
    fn gauge() {
        let config: LogToMetricConfig = toml::from_str(
            r##"
            counters = [{field = "foo", parse_value = true}, {field = "bar", parse_value = false}]

            [[metrics]]
            type = "gauge"
            field = "baz"
            name = "baz"
            labels = {host = "#{event.host}"}
            "##,
        )
        .unwrap();

        let mut log = Event::from("i am a log");
        log.as_mut_log().insert_explicit("baz".into(), "666".into());

        let mut transform = LogToMetric::new(config);

        let metric = transform.transform(log).unwrap();
        assert_eq!(
            metric.into_metric(),
            Metric::Gauge {
                name: "baz".into(),
                val: 666,
                direction: None,
            }
        );
    }

    #[test]
    fn parse_failure() {
        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("foo".into(), "not a number".into());

        let mut transform = LogToMetric::new(config());
        assert_eq!(None, transform.transform(log));
    }

    #[test]
    fn missing_field() {
        let mut log = Event::from("i am a log");
        log.as_mut_log()
            .insert_explicit("not foo".into(), "not a number".into());

        let mut transform = LogToMetric::new(config());
        assert_eq!(None, transform.transform(log));
    }
}
