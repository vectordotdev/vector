#![allow(clippy::redundant_static_lifetimes)]
#![allow(clippy::unreadable_literal)]

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

const INPUTS: &'static [usize] = &[1, 100, 500, 1000];

fn bench(c: &mut Criterion) {
    c.bench_function_over_inputs(
        "No Limit",
        |b, n| {
            let sub = VisitingSubscriber(Mutex::new(String::from("")));
            let n = black_box(n);
            tracing::subscriber::with_default(sub, || {
                b.iter(|| {
                    for _ in 0..**n {
                        info!(
                            message = "hello world",
                            foo = "foo",
                            bar = "bar",
                            baz = 3,
                            quuux = field::debug(0.99),
                        )
                    }
                })
            });
        },
        INPUTS,
    );

    c.bench_function_over_inputs(
        "Limit 5 seconds",
        |b, n| {
            let sub = VisitingSubscriber(Mutex::new(String::from(""))).with(Limit::default());
            let n = black_box(n);
            tracing::subscriber::with_default(sub, || {
                b.iter(|| {
                    for _ in 0..**n {
                        info!(
                            message = "hello world",
                            foo = "foo",
                            bar = "bar",
                            baz = 3,
                            quuux = field::debug(0.99),
                            rate_limit_secs = 5
                        )
                    }
                })
            });
        },
        INPUTS,
    );
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
