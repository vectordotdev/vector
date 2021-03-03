use dashmap::DashMap;
use std::fmt;
use std::time::Instant;
use tracing_core::{
    callsite::Identifier,
    field::{display, Field, Value, Visit},
    span,
    subscriber::Interest,
    Event, Metadata, Subscriber,
};
use tracing_subscriber::layer::{Context, Layer};

#[cfg(test)]
#[macro_use]
extern crate tracing;

const RATE_LIMIT_SECS_FIELD: &str = "internal_log_rate_secs";
const COMPONENT_NAME_FIELD: &str = "component_name";
const MESSAGE_FIELD: &str = "message";

#[derive(Eq, PartialEq, Hash, Clone)]
struct RateKeyIdentifier {
    callsite: Identifier,
    component_name: Option<String>,
}

pub struct RateLimitedLayer<S, L>
where
    L: Layer<S> + Sized,
    S: Subscriber,
{
    events: DashMap<RateKeyIdentifier, State>,
    inner: L,

    _subscriber: std::marker::PhantomData<S>,
}

impl<S, L> RateLimitedLayer<S, L>
where
    L: Layer<S> + Sized,
    S: Subscriber,
{
    pub fn new(layer: L) -> Self {
        RateLimitedLayer {
            events: Default::default(),
            inner: layer,
            _subscriber: std::marker::PhantomData,
        }
    }
}

impl<S, L> Layer<S> for RateLimitedLayer<S, L>
where
    L: Layer<S>,
    S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    #[inline]
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        self.inner.register_callsite(metadata)
    }

    #[inline]
    fn enabled(&self, metadata: &Metadata<'_>, ctx: Context<'_, S>) -> bool {
        self.inner.enabled(metadata, ctx)
    }

    // keep track of any span fields we use for grouping rate limiting
    fn new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        {
            let span = ctx.span(id).expect("Span not found, this is a bug");
            let mut extensions = span.extensions_mut();

            if extensions.get_mut::<RateLimitedSpanKeys>().is_none() {
                let mut fields = RateLimitedSpanKeys::default();
                attrs.record(&mut fields);
                extensions.insert(fields);
            };
        }
        self.inner.new_span(attrs, id, ctx);
    }

    // keep track of any span fields we use for grouping rate limiting
    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        {
            let span = ctx.span(id).expect("Span not found, this is a bug");
            let mut extensions = span.extensions_mut();

            match extensions.get_mut::<RateLimitedSpanKeys>() {
                Some(fields) => {
                    values.record(fields);
                }
                None => {
                    let mut fields = RateLimitedSpanKeys::default();
                    values.record(&mut fields);
                    extensions.insert(fields);
                }
            };
        }
        self.inner.on_record(id, values, ctx);
    }

    #[inline]
    fn on_follows_from(&self, span: &span::Id, follows: &span::Id, ctx: Context<'_, S>) {
        self.inner.on_follows_from(span, follows, ctx);
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let metadata = event.metadata();

        // if the event is not rate limited, just pass through
        if !is_limited(metadata) {
            return self.inner.on_event(event, ctx);
        }

        let mut limit_visitor = LimitVisitor::default();
        event.record(&mut limit_visitor);

        let component_name = {
            let mut scope = ctx
                .lookup_current()
                .into_iter()
                .flat_map(|span| span.from_root().chain(std::iter::once(span)));

            scope.find_map(|span| {
                let extensions = span.extensions();
                extensions
                    .get::<RateLimitedSpanKeys>()
                    .and_then(|fields| fields.component_name.clone())
            })
        };

        let id = RateKeyIdentifier {
            callsite: metadata.callsite(),
            component_name,
        };

        let mut state = self.events.entry(id).or_insert(State {
            start: Instant::now(),
            count: 0,
            // if this is None, then a non-integer was passed as the rate limit
            limit: limit_visitor
                .limit
                .expect("unreachable; if you see this, there is a bug"),
            message: limit_visitor
                .message
                .unwrap_or_else(|| event.metadata().name().into()),
        });

        let prev = state.count;
        state.count += 1;

        //check if we are still rate limiting
        if state.start.elapsed().as_secs() < state.limit.get() {
            // check and increment the current count
            // if 0: this is the first message, just pass it through
            // if 1: this is the first rate limited message
            // otherwise supress it until the rate limit expires
            match prev {
                0 => self.inner.on_event(event, ctx),
                1 => {
                    // output first rate limited log
                    let message =
                        format!("Internal log [{}] is being rate limited.", state.message);
                    self.create_event(&ctx, metadata, message, state.limit);
                }
                _ => {}
            }
        } else {
            // done rate limiting

            // output a message if any events were rate limited
            if prev > 1 {
                let message = format!(
                    "Internal log [{}] has been rate limited {} times.",
                    state.message,
                    prev - 1
                );

                self.create_event(&ctx, metadata, message, state.limit);
            }
            self.inner.on_event(event, ctx);
            state.start = Instant::now();
            // we emitted the event, so the next one within `limit` should be rate limited
            state.count = 1;
        }
    }

    #[inline]
    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        self.inner.on_enter(id, ctx);
    }

    #[inline]
    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        self.inner.on_exit(id, ctx);
    }

    #[inline]
    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        self.inner.on_close(id, ctx);
    }

    #[inline]
    fn on_id_change(&self, old: &span::Id, new: &span::Id, ctx: Context<'_, S>) {
        self.inner.on_id_change(old, new, ctx);
    }
}

impl<S, L> RateLimitedLayer<S, L>
where
    S: Subscriber,
    L: Layer<S>,
{
    fn create_event(
        &self,
        ctx: &Context<S>,
        metadata: &'static Metadata<'static>,
        message: String,
        rate_limit: std::num::NonZeroU64,
    ) {
        let fields = metadata.fields();

        let message = display(message);

        if let Some(message_field) = fields.field("message") {
            let values = [(&message_field, Some(&message as &dyn Value))];

            let valueset = fields.value_set(&values);
            let event = Event::new(metadata, &valueset);
            self.inner.on_event(&event, ctx.clone());
        } else {
            let values = [(
                &fields.field(RATE_LIMIT_SECS_FIELD).unwrap(),
                Some(&rate_limit as &dyn Value),
            )];

            let valueset = fields.value_set(&values);
            let event = Event::new(metadata, &valueset);
            self.inner.on_event(&event, ctx.clone());
        }
    }
}

#[derive(Debug)]
struct State {
    start: Instant,
    count: u64,
    limit: std::num::NonZeroU64,
    message: String,
}

fn is_limited(metadata: &Metadata<'_>) -> bool {
    metadata
        .fields()
        .iter()
        .any(|f| f.name() == RATE_LIMIT_SECS_FIELD)
}

/// RateLimitedSpanKeys records span keys that we use to rate limit callsites separately by. For
/// example, if a given trace callsite is called from two different components, then they will be
/// rate limited separately.
#[derive(Default)]
struct RateLimitedSpanKeys {
    pub component_name: Option<String>,
}

impl Visit for RateLimitedSpanKeys {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == COMPONENT_NAME_FIELD {
            self.component_name = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {}
}

#[derive(Default)]
struct LimitVisitor {
    pub limit: Option<std::num::NonZeroU64>,
    pub message: Option<String>,
}

impl Visit for LimitVisitor {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == RATE_LIMIT_SECS_FIELD {
            self.limit = std::num::NonZeroU64::new(value);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == RATE_LIMIT_SECS_FIELD {
            use std::convert::TryFrom;
            self.limit = std::num::NonZeroU64::new(
                u64::try_from(value).expect("rate-limit must not be negative"),
            );
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == MESSAGE_FIELD {
            self.message = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == MESSAGE_FIELD {
            self.message = Some(format!("{:?}", value));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    #[derive(Default)]
    struct RecordingLayer<S> {
        events: Arc<Mutex<Vec<String>>>,

        _subscriber: std::marker::PhantomData<S>,
    }

    impl<S> RecordingLayer<S> {
        fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
            RecordingLayer {
                events,

                _subscriber: std::marker::PhantomData,
            }
        }
    }

    impl<S> Layer<S> for RecordingLayer<S>
    where
        S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    {
        fn register_callsite(&self, _metadata: &'static Metadata<'static>) -> Interest {
            Interest::always()
        }

        fn enabled(&self, _metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
            true
        }

        fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
            let mut visitor = LimitVisitor::default();
            event.record(&mut visitor);

            let mut events = self.events.lock().unwrap();
            events.push(visitor.message.unwrap_or_default());
        }
    }

    #[test]
    fn rate_limits() {
        let events: Arc<Mutex<Vec<String>>> = Default::default();

        let recorder = RecordingLayer::new(Arc::clone(&events));
        let sub =
            tracing_subscriber::registry::Registry::default().with(RateLimitedLayer::new(recorder));
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                info!(message = "Hello world!", internal_log_rate_secs = 1);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        assert_eq!(
            *events,
            vec![
                "Hello world!",
                "Internal log [Hello world!] is being rate limited.",
                "Internal log [Hello world!] has been rate limited 9 times.",
                "Hello world!",
                "Internal log [Hello world!] is being rate limited.",
                "Internal log [Hello world!] has been rate limited 9 times.",
                "Hello world!",
            ]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<String>>()
        );
    }

    #[test]
    fn rate_limit_by_component_name() {
        let events: Arc<Mutex<Vec<String>>> = Default::default();

        let recorder = RecordingLayer::new(Arc::clone(&events));
        let sub =
            tracing_subscriber::registry::Registry::default().with(RateLimitedLayer::new(recorder));
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                for key in &["foo", "bar"] {
                    let span = info_span!("span", component_name = &key);
                    let _enter = span.enter();
                    info!(
                        message = format!("Hello {}!", key).as_str(),
                        internal_log_rate_secs = 1
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        dbg!(&events);

        assert_eq!(
            *events,
            vec![
                "Hello foo!",
                "Hello bar!",
                "Internal log [Hello foo!] is being rate limited.",
                "Internal log [Hello bar!] is being rate limited.",
                "Internal log [Hello foo!] has been rate limited 9 times.",
                "Hello foo!",
                "Internal log [Hello bar!] has been rate limited 9 times.",
                "Hello bar!",
                "Internal log [Hello foo!] is being rate limited.",
                "Internal log [Hello bar!] is being rate limited.",
                "Internal log [Hello foo!] has been rate limited 9 times.",
                "Hello foo!",
                "Internal log [Hello bar!] has been rate limited 9 times.",
                "Hello bar!",
            ]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<String>>()
        );
    }
}
