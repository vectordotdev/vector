use std::fmt::{Display, Formatter};

use bytes::BytesMut;
use vector_core::event::{Event, MetricKind};
use vector_core::EstimatedJsonEncodedSizeOf;

use crate::components::validation::{
    encode_test_event, ComponentConfiguration, ResourceCodec, TestEvent,
};

use crate::components::validation::runner::config::TEST_SOURCE_NAME;
use crate::sources::Sources;

use super::filter_events_by_metric_and_component;

pub enum SourceMetrics {
    EventsReceived,
    EventsReceivedBytes,
    ReceivedBytesTotal,
    SentEventsTotal,
    SentEventBytesTotal,
}

impl SourceMetrics {
    const fn name(&self) -> &'static str {
        match self {
            SourceMetrics::EventsReceived => "component_received_events_total",
            SourceMetrics::EventsReceivedBytes => "component_received_event_bytes_total",
            SourceMetrics::ReceivedBytesTotal => "component_received_bytes_total",
            SourceMetrics::SentEventsTotal => "component_sent_events_total",
            SourceMetrics::SentEventBytesTotal => "component_sent_event_bytes_total",
        }
    }

    pub const fn validation(
        &self,
    ) -> fn(
        &ComponentConfiguration,
        &[TestEvent],
        &[Event],
        &[Event],
    ) -> Result<Vec<String>, Vec<String>> {
        match self {
            SourceMetrics::EventsReceived => validate_component_received_events_total,
            SourceMetrics::EventsReceivedBytes => validate_component_received_event_bytes_total,
            SourceMetrics::ReceivedBytesTotal => validate_component_received_bytes_total,
            SourceMetrics::SentEventsTotal => validate_component_sent_events_total,
            SourceMetrics::SentEventBytesTotal => validate_component_sent_event_bytes_total,
        }
    }

    pub const fn all() -> [SourceMetrics; 5] {
        [
            SourceMetrics::EventsReceived,
            SourceMetrics::EventsReceivedBytes,
            SourceMetrics::ReceivedBytesTotal,
            SourceMetrics::SentEventsTotal,
            SourceMetrics::SentEventBytesTotal,
        ]
    }
}

impl Display for SourceMetrics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

fn validate_component_received_events_total(
    _configuration: &ComponentConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::EventsReceived,
        TEST_SOURCE_NAME,
    )?;

    let mut events: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    events = *value
                } else {
                    events += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::EventsReceived,
            )),
        }
    }

    let expected_events = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(_) = i {
            return acc + 1;
        }
        acc
    });

    debug!(
        "{}: {} events, {} expected events",
        SourceMetrics::EventsReceived,
        events,
        expected_events,
    );

    if events != expected_events as f64 {
        errs.push(format!(
            "{}: expected {} events, but received {}",
            SourceMetrics::EventsReceived,
            expected_events,
            events
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::EventsReceived,
        events,
    )])
}

fn validate_component_received_event_bytes_total(
    _configuration: &ComponentConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::EventsReceivedBytes,
        TEST_SOURCE_NAME,
    )?;

    let mut metric_bytes: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    metric_bytes = *value
                } else {
                    metric_bytes += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::EventsReceivedBytes,
            )),
        }
    }

    let expected_bytes = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(_) = i {
            let size = vec![i.clone().into_event()].estimated_json_encoded_size_of();
            return acc + size;
        }

        // If we don't have a valid event, we'll just add the JSON length of an empty container,
        // like []
        acc + 2
    });

    debug!(
        "{}: {} bytes, {} expected bytes",
        SourceMetrics::EventsReceivedBytes,
        metric_bytes,
        expected_bytes,
    );

    if metric_bytes != expected_bytes as f64 {
        errs.push(format!(
            "{}: expected {} bytes, but received {}",
            SourceMetrics::EventsReceivedBytes,
            expected_bytes,
            metric_bytes
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::EventsReceivedBytes,
        metric_bytes,
    )])
}

fn validate_component_received_bytes_total(
    configuration: &ComponentConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::ReceivedBytesTotal,
        TEST_SOURCE_NAME,
    )?;

    let mut metric_bytes: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    metric_bytes = *value
                } else {
                    metric_bytes += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::ReceivedBytesTotal,
            )),
        }
    }

    let mut expected_bytes = 0;

    // TODO: this is a bit of a hack
    if let ComponentConfiguration::Source(Sources::HttpClient(c)) = configuration {
        let mut encoder = ResourceCodec::from(c.get_decoding_config(None)).into_encoder();

        for i in inputs {
            let mut buffer = BytesMut::new();
            encode_test_event(&mut encoder, &mut buffer, i.clone());
            expected_bytes += buffer.len()
        }
    }

    debug!(
        "{}: {} bytes, {} expected bytes",
        SourceMetrics::ReceivedBytesTotal,
        metric_bytes,
        expected_bytes,
    );

    if metric_bytes != expected_bytes as f64 {
        errs.push(format!(
            "{}: expected {} bytes, but received {}",
            SourceMetrics::ReceivedBytesTotal,
            expected_bytes,
            metric_bytes
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::ReceivedBytesTotal,
        metric_bytes,
    )])
}

fn validate_component_sent_events_total(
    _configuration: &ComponentConfiguration,
    inputs: &[TestEvent],
    _outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::SentEventsTotal,
        TEST_SOURCE_NAME,
    )?;

    let mut events: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    events = *value
                } else {
                    events += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::SentEventsTotal,
            )),
        }
    }

    let expected_events = inputs.iter().fold(0, |acc, i| {
        if let TestEvent::Passthrough(_) = i {
            return acc + 1;
        }
        acc
    });

    debug!(
        "{}: {} events, {} expected events",
        SourceMetrics::SentEventsTotal,
        events,
        expected_events,
    );

    if events != expected_events as f64 {
        errs.push(format!(
            "{}: expected {} events, but received {}",
            SourceMetrics::SentEventsTotal,
            inputs.len(),
            events
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::SentEventsTotal,
        events,
    )])
}

fn validate_component_sent_event_bytes_total(
    _configuration: &ComponentConfiguration,
    _inputs: &[TestEvent],
    outputs: &[Event],
    telemetry_events: &[Event],
) -> Result<Vec<String>, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    let metrics = filter_events_by_metric_and_component(
        telemetry_events,
        SourceMetrics::SentEventBytesTotal,
        TEST_SOURCE_NAME,
    )?;

    let mut metric_bytes: f64 = 0.0;
    for m in metrics {
        match m.value() {
            vector_core::event::MetricValue::Counter { value } => {
                if let MetricKind::Absolute = m.data().kind {
                    metric_bytes = *value
                } else {
                    metric_bytes += value
                }
            }
            _ => errs.push(format!(
                "{}: metric value is not a counter",
                SourceMetrics::SentEventBytesTotal,
            )),
        }
    }

    let mut expected_bytes = 0;
    for e in outputs {
        expected_bytes += vec![e].estimated_json_encoded_size_of();
    }

    debug!(
        "{}: {} bytes, {} expected bytes",
        SourceMetrics::SentEventBytesTotal,
        metric_bytes,
        expected_bytes,
    );

    if metric_bytes != expected_bytes as f64 {
        errs.push(format!(
            "{}: expected {} bytes, but received {}",
            SourceMetrics::SentEventBytesTotal,
            expected_bytes,
            metric_bytes
        ));
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    Ok(vec![format!(
        "{}: {}",
        SourceMetrics::SentEventBytesTotal,
        metric_bytes,
    )])
}
