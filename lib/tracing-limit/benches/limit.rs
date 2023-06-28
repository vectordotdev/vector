#[macro_use]
extern crate tracing;

#[macro_use]
extern crate criterion;

use std::{
    fmt,
    sync::{Mutex, MutexGuard},
};

use criterion::{black_box, BenchmarkId, Criterion};
use tracing::{field, span, subscriber::Interest, Event, Metadata, Subscriber};
use tracing_limit::RateLimitedLayer;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

const INPUTS: &[usize] = &[1, 100, 500, 1000];

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("tracing-limit");
    for input in INPUTS {
        group.bench_with_input(
            BenchmarkId::new("none", input.to_string()),
            input,
            |b, n| {
                let sub = tracing_subscriber::registry::Registry::default().with(
                    RateLimitedLayer::new(VisitingLayer::new(Mutex::new(String::from("")))),
                );
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
            },
        );
    }

    for input in INPUTS {
        group.bench_with_input(BenchmarkId::new("5s", input.to_string()), input, |b, n| {
            let sub = tracing_subscriber::registry::Registry::default().with(
                RateLimitedLayer::new(VisitingLayer::new(Mutex::new(String::from("")))),
            );
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
                            internal_log_rate_limit = true
                        )
                    }
                })
            });
        });
    }
    group.finish();
}

/// Simulates a layer that records span data.
struct VisitingLayer<S>
where
    S: Subscriber,
{
    mutex: Mutex<String>,

    _subscriber: std::marker::PhantomData<S>,
}

impl<S> VisitingLayer<S>
where
    S: Subscriber,
{
    fn new(mutex: Mutex<String>) -> Self {
        VisitingLayer {
            mutex,

            _subscriber: std::marker::PhantomData,
        }
    }
}

struct Visitor<'a>(MutexGuard<'a, String>);

impl<'a> field::Visit for Visitor<'a> {
    fn record_debug(&mut self, _field: &field::Field, value: &dyn fmt::Debug) {
        use std::fmt::Write;
        _ = write!(&mut *self.0, "{:?}", value);
    }
}

impl<S> Layer<S> for VisitingLayer<S>
where
    S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn register_callsite(&self, _metadata: &'static Metadata<'static>) -> Interest {
        Interest::always()
    }

    fn enabled(&self, metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        _ = metadata;
        true
    }

    fn on_new_span(&self, span: &span::Attributes<'_>, _id: &span::Id, _ctx: Context<'_, S>) {
        let mut visitor = Visitor(self.mutex.lock().unwrap());
        span.record(&mut visitor);
    }

    fn on_record(&self, _id: &span::Id, values: &span::Record<'_>, _ctx: Context<'_, S>) {
        let mut visitor = Visitor(self.mutex.lock().unwrap());
        values.record(&mut visitor);
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = Visitor(self.mutex.lock().unwrap());
        event.record(&mut visitor);
    }

    fn on_follows_from(&self, id: &span::Id, follows: &span::Id, _ctx: Context<'_, S>) {
        _ = (id, follows);
    }

    fn on_enter(&self, id: &span::Id, _ctx: Context<'_, S>) {
        _ = id;
    }

    fn on_exit(&self, id: &span::Id, _ctx: Context<'_, S>) {
        _ = id;
    }

    fn on_close(&self, id: span::Id, _ctx: Context<'_, S>) {
        _ = id;
    }

    fn on_id_change(&self, old: &span::Id, new: &span::Id, _ctx: Context<'_, S>) {
        _ = (old, new);
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
