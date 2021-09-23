#![deny(missing_docs)]
use crate::event::Metric;
use crate::metrics::Controller;

/// Tests if the given metric contains all the given tag names
fn has_tags(metric: &Metric, names: &[&str]) -> bool {
    metric
        .tags()
        .map(|tags| names.iter().all(|name| tags.contains_key(*name)))
        .unwrap_or_else(|| names.is_empty())
}

/// Standard metrics test environment data
struct MetricsTest {
    metrics: Vec<Metric>,
    errors: Vec<String>,
}

impl MetricsTest {
    fn new() -> Self {
        let mut metrics: Vec<_> = Controller::get().unwrap().capture_metrics().collect();
        metrics.sort_by(|a, b| a.name().cmp(&b.name()));
        for metric in &metrics {
            println!("{}", metric);
        }
        let errors = Vec::new();
        Self { metrics, errors }
    }

    fn has_all_metrics(&mut self, names: &[&str], tags: &[&str]) {
        let tag_suffix = (!tags.is_empty())
            .then(|| format!("{{{}}}", tags.join(",")))
            .unwrap_or_else(|| String::new());
        for name in names {
            if !self
                .metrics
                .iter()
                .any(|m| m.name() == *name && has_tags(m, tags))
            {
                self.errors
                    .push(format!("Missing metric named {}{}", name, tag_suffix));
            }
        }
    }

    fn result(self) -> Result<(), Vec<String>> {
        self.errors
            .is_empty()
            .then(|| Ok(()))
            .unwrap_or_else(|| Err(self.errors))
    }
}

/// Test if the source that just run has emitted the standard source-type events.
pub(crate) fn emits_source_events(tags: &[&str]) -> Result<(), Vec<String>> {
    let mut test = MetricsTest::new();

    test.has_all_metrics(
        &[
            "component_received_bytes_total",
            "component_received_events_total",
            "component_received_event_bytes_total",
        ],
        tags,
    );

    test.has_all_metrics(
        &[
            "component_sent_events_total",
            "component_sent_event_bytes_total",
        ],
        &[],
    );
    test.result()
}
