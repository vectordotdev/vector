use super::Transform;
use crate::{
    event::metric::Metric,
    template::Template,
    topology::config::{DataType, TransformConfig},
    Event,
};
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
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct GaugeConfig {
    field: Atom,
    #[serde(skip)]
    sanitized_name: Atom,
    name: Option<Atom>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct SetConfig {
    field: Atom,
    #[serde(skip)]
    sanitized_name: Atom,
    name: Option<Atom>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct HistogramConfig {
    field: Atom,
    #[serde(skip)]
    sanitized_name: Atom,
    name: Option<Atom>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MetricConfig {
    Counter(CounterConfig),
    Histogram(HistogramConfig),
    Gauge(GaugeConfig),
    Set(SetConfig),
}

fn default_increment_by_value() -> bool {
    false
}

pub struct LogToMetric {
    config: LogToMetricConfig,
}

#[typetag::serde(name = "log_to_metric")]
impl TransformConfig for LogToMetricConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(LogToMetric::new(self)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
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
                MetricConfig::Histogram(ref mut hist) => {
                    let name = hist.name.as_ref().unwrap_or(&hist.field).to_string();
                    hist.sanitized_name = Atom::from(name);
                }
                MetricConfig::Gauge(ref mut gauge) => {
                    let name = gauge.name.as_ref().unwrap_or(&gauge.field).to_string();
                    gauge.sanitized_name = Atom::from(name);
                }
                MetricConfig::Set(ref mut set) => {
                    let name = set.name.as_ref().unwrap_or(&set.field).to_string();
                    set.sanitized_name = Atom::from(name);
                }
            }
        }

        LogToMetric { config }
    }
}

fn render_template(s: &str, event: &Event) -> Result<String, String> {
    let template = Template::from(s);
    let name = template
        .render(&event)
        .map_err(|e| format!("Keys ({:?}) do not exist on the event. Dropping event.", e))?;
    Ok(String::from_utf8_lossy(&name.to_vec()).to_string())
}

fn to_metric(config: &MetricConfig, event: &Event) -> Result<Metric, String> {
    let log = event.as_log();
    match config {
        MetricConfig::Counter(counter) => {
            let val = log.get(&counter.field).ok_or("")?;
            let val = if counter.increment_by_value {
                val.to_string_lossy()
                    .parse()
                    .map_err(|_| "failed to parse counter value".to_string())?
            } else {
                1.0
            };

            let name = render_template(&counter.sanitized_name, &event)?;

            Ok(Metric::Counter { name, val })
        }
        MetricConfig::Histogram(hist) => {
            let val = log.get(&hist.field).ok_or("")?;
            let val = val
                .to_string_lossy()
                .parse()
                .map_err(|_| "failed to parse histogram value".to_string())?;

            let name = render_template(&hist.sanitized_name, &event)?;

            Ok(Metric::Histogram {
                name,
                val,
                sample_rate: 1,
            })
        }
        MetricConfig::Gauge(gauge) => {
            let val = log.get(&gauge.field).ok_or("")?;
            let val = val
                .to_string_lossy()
                .parse()
                .map_err(|_| "failed to parse gauge value".to_string())?;

            let name = render_template(&gauge.sanitized_name, &event)?;

            Ok(Metric::Gauge {
                name,
                val,
                direction: None,
            })
        }
        MetricConfig::Set(set) => {
            let val = log.get(&set.field).ok_or("")?;
            let val = val.to_string_lossy();

            let name = render_template(&set.sanitized_name, &event)?;

            Ok(Metric::Set { name, val })
        }
    }
}

impl Transform for LogToMetric {
    // Only used in tests
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut output = Vec::new();
        self.transform_into(&mut output, event);
        output.pop()
    }

    fn transform_into(&mut self, output: &mut Vec<Event>, event: Event) {
        for config in self.config.metrics.iter() {
            match to_metric(&config, &event) {
                Ok(metric) => {
                    output.push(Event::Metric(metric));
                }
                Err(message) => {
                    if !message.is_empty() {
                        trace!("{:?}", message);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LogToMetric, LogToMetricConfig};
    use crate::{event::metric::Metric, transforms::Transform, Event};

    fn parse_config(s: &str) -> LogToMetricConfig {
        toml::from_str(s).unwrap()
    }

    fn create_event(key: &str, value: &str) -> Event {
        let mut log = Event::from("i am a log");
        log.as_mut_log().insert_explicit(key.into(), value.into());
        log
    }

    #[test]
    fn count_http_status_codes() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "status"
            "#,
        );

        let event = create_event("status", "42");
        let mut transform = LogToMetric::new(&config);
        let metric = transform.transform(event).unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::Counter {
                name: "status_total".into(),
                val: 1.0,
            }
        );
    }

    #[test]
    fn count_exceptions() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "exception_total"
            "#,
        );

        let event = create_event("backtrace", "message");
        let mut transform = LogToMetric::new(&config);
        let metric = transform.transform(event).unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::Counter {
                name: "exception_total".into(),
                val: 1.0,
            }
        );
    }

    #[test]
    fn count_exceptions_no_match() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "exception_total"
            "#,
        );

        let event = create_event("success", "42");
        let mut transform = LogToMetric::new(&config);
        let metric = transform.transform(event);

        assert!(metric.is_none());
    }

    #[test]
    fn sum_order_amounts() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "amount"
            name = "amount_total"
            increment_by_value = true
            "#,
        );

        let event = create_event("amount", "33.99");
        let mut transform = LogToMetric::new(&config);
        let metric = transform.transform(event).unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::Counter {
                name: "amount_total".into(),
                val: 33.99,
            }
        );
    }

    #[test]
    fn memory_usage_gauge() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "gauge"
            field = "memory_rss"
            name = "memory_rss_bytes"
            "#,
        );

        let event = create_event("memory_rss", "123");
        let mut transform = LogToMetric::new(&config);
        let metric = transform.transform(event).unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::Gauge {
                name: "memory_rss_bytes".into(),
                val: 123.0,
                direction: None,
            }
        );
    }

    #[test]
    fn parse_failure() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "status"
            name = "status_total"
            increment_by_value = true
            "#,
        );

        let event = create_event("status", "not a number");
        let mut transform = LogToMetric::new(&config);

        assert!(transform.transform(event).is_none());
    }

    #[test]
    fn missing_field() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "status"
            name = "status_total"
            "#,
        );

        let event = create_event("not foo", "not a number");
        let mut transform = LogToMetric::new(&config);

        assert!(transform.transform(event).is_none());
    }

    #[test]
    fn multiple_metrics() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "counter"
            field = "status"

            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "exception_total"
            "#,
        );

        let mut event = Event::from("i am a log");
        event
            .as_mut_log()
            .insert_explicit("status".into(), "42".into());
        event
            .as_mut_log()
            .insert_explicit("backtrace".into(), "message".into());

        let mut transform = LogToMetric::new(&config);

        let mut output = Vec::new();
        transform.transform_into(&mut output, event);
        assert_eq!(2, output.len());
        assert_eq!(
            output.pop().unwrap().into_metric(),
            Metric::Counter {
                name: "exception_total".into(),
                val: 1.0,
            }
        );
        assert_eq!(
            output.pop().unwrap().into_metric(),
            Metric::Counter {
                name: "status_total".into(),
                val: 1.0,
            }
        );
    }

    #[test]
    fn multiple_metrics_with_multiple_templates() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "set"
            field = "status"
            name = "{{host}}_{{worker}}_status_set"

            [[metrics]]
            type = "counter"
            field = "backtrace"
            name = "{{service}}_exception_total"
            "#,
        );

        let mut event = Event::from("i am a log");
        event
            .as_mut_log()
            .insert_explicit("status".into(), "42".into());
        event
            .as_mut_log()
            .insert_explicit("backtrace".into(), "message".into());
        event
            .as_mut_log()
            .insert_implicit("host".into(), "local".into());
        event
            .as_mut_log()
            .insert_implicit("worker".into(), "abc".into());
        event
            .as_mut_log()
            .insert_implicit("service".into(), "xyz".into());

        let mut transform = LogToMetric::new(&config);

        let mut output = Vec::new();
        transform.transform_into(&mut output, event);
        assert_eq!(2, output.len());
        assert_eq!(
            output.pop().unwrap().into_metric(),
            Metric::Counter {
                name: "xyz_exception_total".into(),
                val: 1.0,
            }
        );
        assert_eq!(
            output.pop().unwrap().into_metric(),
            Metric::Set {
                name: "local_abc_status_set".into(),
                val: "42".into(),
            }
        );
    }

    #[test]
    fn user_ip_set() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "set"
            field = "user_ip"
            name = "unique_user_ip"
            "#,
        );

        let event = create_event("user_ip", "1.2.3.4");
        let mut transform = LogToMetric::new(&config);
        let metric = transform.transform(event).unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::Set {
                name: "unique_user_ip".into(),
                val: "1.2.3.4".into(),
            }
        );
    }

    #[test]
    fn response_time_histogram() {
        let config = parse_config(
            r#"
            [[metrics]]
            type = "histogram"
            field = "response_time"
            "#,
        );

        let event = create_event("response_time", "2.5");
        let mut transform = LogToMetric::new(&config);
        let metric = transform.transform(event).unwrap();

        assert_eq!(
            metric.into_metric(),
            Metric::Histogram {
                name: "response_time".into(),
                val: 2.5,
                sample_rate: 1,
            }
        );
    }
}
