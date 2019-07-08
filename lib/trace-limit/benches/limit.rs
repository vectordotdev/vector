#[macro_use]
extern crate tokio_trace;

#[macro_use]
extern crate criterion;

use criterion::{black_box, Criterion};

use std::{
    fmt,
    sync::{Mutex, MutexGuard},
};
use tokio_trace::{field, span, Event, Id, Metadata};
use trace_limit::LimitSubscriber;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("basline_record", |b| {
        let sub = VisitingSubscriber(Mutex::new(String::from("")));
        let n = black_box(5000);
        tokio_trace::subscriber::with_default(sub, || {
            b.iter(|| {
                for _ in 0..n {
                    info!(
                        message = "hello world",
                        foo = "foo",
                        bar = "bar",
                        baz = 3,
                        quuux = tokio_trace::field::debug(0.99)
                    )
                }
            })
        });
    });

    c.bench_function("limit_record_5", |b| {
        let sub = LimitSubscriber::new(VisitingSubscriber(Mutex::new(String::from(""))));
        let n = black_box(5000);
        tokio_trace::subscriber::with_default(sub, || {
            b.iter(|| {
                for _ in 0..n {
                    info!(
                        message = "hello world",
                        foo = "foo",
                        bar = "bar",
                        baz = 3,
                        quuux = tokio_trace::field::debug(0.99),
                        rate_limit = 5
                    )
                }
            })
        });
    });

    c.bench_function("limit_record_100", |b| {
        let sub = LimitSubscriber::new(VisitingSubscriber(Mutex::new(String::from(""))));
        let n = black_box(5000);
        tokio_trace::subscriber::with_default(sub, || {
            b.iter(|| {
                for _ in 0..n {
                    info!(
                        message = "hello world",
                        foo = "foo",
                        bar = "bar",
                        baz = 3,
                        quuux = tokio_trace::field::debug(0.99),
                        rate_limit = 100
                    )
                }
            })
        });
    });

    c.bench_function("limit_record_1000", |b| {
        let sub = LimitSubscriber::new(VisitingSubscriber(Mutex::new(String::from(""))));
        let n = black_box(5000);
        tokio_trace::subscriber::with_default(sub, || {
            b.iter(|| {
                for _ in 0..n {
                    info!(
                        message = "hello world",
                        foo = "foo",
                        bar = "bar",
                        baz = 3,
                        quuux = tokio_trace::field::debug(0.99),
                        rate_limit = 1000
                    )
                }
            })
        });
    });
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

impl tokio_trace::Subscriber for VisitingSubscriber {
    fn new_span(&self, span: &span::Attributes) -> Id {
        let mut visitor = Visitor(self.0.lock().unwrap());
        span.record(&mut visitor);
        Id::from_u64(0xDEADFACE)
    }

    fn record(&self, _span: &Id, values: &span::Record) {
        let mut visitor = Visitor(self.0.lock().unwrap());
        values.record(&mut visitor);
    }

    fn event(&self, event: &Event) {
        let mut visitor = Visitor(self.0.lock().unwrap());
        event.record(&mut visitor);
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        let _ = (span, follows);
    }

    fn enabled(&self, metadata: &Metadata) -> bool {
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

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
