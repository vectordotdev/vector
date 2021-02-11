#[macro_use]
extern crate tracing;

#[macro_use]
extern crate criterion;

use criterion::{black_box, Criterion};
use std::{
    fmt,
    sync::{Mutex, MutexGuard},
};
use tracing::{field, span, Event, Id, Metadata};
use tracing_limit::Limit;
use tracing_subscriber::layer::SubscriberExt;

const INPUTS: &[usize] = &[1, 100, 500, 1000];

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("No Limit");
    for input in INPUTS {
        group.bench_with_input(input.to_string(), input, |b, n| {
            let sub = VisitingSubscriber(Mutex::new(String::from("")));
            let n = black_box(n);
            tracing::subscriber::with_default(sub, || {
                b.iter(|| {
                    for _ in 0..*n {
                        info!(
                            message = "Hello world!",
                            foo = "foo",
                            bar = "bar",
                            baz = 3,
                            quuux = ?0.99,
                        )
                    }
                })
            });
        });
    }
    group.finish();

    let mut group = c.benchmark_group("Limit 5 seconds");
    for input in INPUTS {
        group.bench_with_input(input.to_string(), input, |b, n| {
            let sub = VisitingSubscriber(Mutex::new(String::from(""))).with(Limit::default());
            let n = black_box(n);
            tracing::subscriber::with_default(sub, || {
                b.iter(|| {
                    for _ in 0..*n {
                        info!(
                            message = "Hello world!",
                            foo = "foo",
                            bar = "bar",
                            baz = 3,
                            quuux = ?0.99,
                            internal_log_rate_secs = 5
                        )
                    }
                })
            });
        });
    }
    group.finish();
}

/// Simulates a subscriber that records span data.
struct VisitingSubscriber(Mutex<String>);

struct Visitor<'a>(MutexGuard<'a, String>);

impl<'a> field::Visit for Visitor<'a> {
    fn record_debug(&mut self, _field: &field::Field, value: &dyn fmt::Debug) {
        use std::fmt::Write;
        let _ = write!(&mut *self.0, "{:?}", value);
    }
}

impl tracing::Subscriber for VisitingSubscriber {
    fn new_span(&self, span: &span::Attributes<'_>) -> Id {
        let mut visitor = Visitor(self.0.lock().unwrap());
        span.record(&mut visitor);
        Id::from_u64(0xDEADFACE)
    }

    fn record(&self, _span: &Id, values: &span::Record<'_>) {
        let mut visitor = Visitor(self.0.lock().unwrap());
        values.record(&mut visitor);
    }

    fn event(&self, event: &Event<'_>) {
        let mut visitor = Visitor(self.0.lock().unwrap());
        event.record(&mut visitor);
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        let _ = (span, follows);
    }

    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        let _ = metadata;
        true
    }

    fn enter(&self, span: &Id) {
        let _ = span;
    }

    fn exit(&self, span: &Id) {
        let _ = span;
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
