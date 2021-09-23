#![deny(missing_docs)]
use crate::event::{Metric, MetricValue};
use crate::metrics::Controller;
use lazy_static::lazy_static;
use std::cell::RefCell;
use std::env;

pub struct ComponentTest {
    events: &'static [&'static str],
    tagged_counters: &'static [&'static str],
    untagged_counters: &'static [&'static str],
}

lazy_static! {
    pub(crate) static ref SOURCE_TEST: ComponentTest = ComponentTest {
        events: &["BytesReceived", "EventsReceived", "EventsSent"],
        tagged_counters: &[
            "component_received_bytes_total",
            "component_received_events_total",
            "component_received_event_bytes_total",
        ],
        untagged_counters: &[
            "component_sent_events_total",
            "component_sent_event_bytes_total",
        ],
    };
}

impl ComponentTest {
    pub(crate) fn assert(&self, tags: &[&str]) {
        let mut test = ComponentTester::new();
        test.emitted_all_events(self.events);
        test.emitted_all_counters(self.tagged_counters, tags);
        test.emitted_all_counters(self.untagged_counters, &[]);
        if let Err(errors) = test.result() {
            panic!(
                "Failed to assert compliance, errors:\n    {}\n",
                errors.join("\n    ")
            );
        }
    }
}

thread_local!(static EVENTS_RECORDED: RefCell<Vec<String>> = RefCell::new(Vec::new()));

pub(crate) fn init() {
    crate::metrics::init_test().expect("Failed to initialize metrics recorder");
}

pub(crate) fn record_internal_event(s: impl Into<String>) {
    let s = s.into();
    if dump_data() {
        println!("{}", s);
    }
    EVENTS_RECORDED.with(move |er| er.borrow_mut().push(s));
}

fn dump_data() -> bool {
    env::var("DEBUG_COMPLIANCE").is_ok()
}

fn event_base_name(mut event: &str) -> &str {
    if event.starts_with('&') {
        event = &event[1..];
    }
    if let Some(par) = event.find('{') {
        event = &event[..par];
    }
    event
}

/// Tests if the given metric contains all the given tag names
fn has_tags(metric: &Metric, names: &[&str]) -> bool {
    metric
        .tags()
        .map(|tags| names.iter().all(|name| tags.contains_key(*name)))
        .unwrap_or_else(|| names.is_empty())
}

/// Standard metrics test environment data
struct ComponentTester {
    metrics: Vec<Metric>,
    errors: Vec<String>,
}

impl ComponentTester {
    fn new() -> Self {
        let mut metrics: Vec<_> = Controller::get().unwrap().capture_metrics().collect();
        if dump_data() {
            metrics.sort_by(|a, b| a.name().cmp(&b.name()));
            for metric in &metrics {
                println!("{}", metric);
            }
        }
        let errors = Vec::new();
        Self { metrics, errors }
    }

    fn emitted_all_counters(&mut self, names: &[&str], tags: &[&str]) {
        let tag_suffix = (!tags.is_empty())
            .then(|| format!("{{{}}}", tags.join(",")))
            .unwrap_or_else(|| String::new());
        for name in names {
            if !self.metrics.iter().any(|m| {
                matches!(m.value(), MetricValue::Counter { .. })
                    && m.name() == *name
                    && has_tags(m, tags)
            }) {
                self.errors
                    .push(format!("Missing metric named {}{}", name, tag_suffix));
            }
        }
    }

    fn emitted_all_events(&mut self, names: &[&str]) {
        for name in names {
            if !EVENTS_RECORDED.with(|events| {
                events
                    .borrow()
                    .iter()
                    .any(|event| event_base_name(event).ends_with(name))
            }) {
                self.errors.push(format!("Missing emitted event {}", name));
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
