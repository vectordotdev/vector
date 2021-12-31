use std::fmt;

use dashmap::DashMap;
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

#[cfg(not(test))]
use std::time::Instant;

#[cfg(test)]
use mock_instant::Instant;

const RATE_LIMIT_SECS_FIELD: &str = "internal_log_rate_secs";
const MESSAGE_FIELD: &str = "message";

// These fields will cause events to be independently rate limited by the values
// for these keys
const COMPONENT_ID_FIELD: &str = "component_id";
const VRL_LINE_NUMBER: &str = "vrl_line_number";
const VRL_POSITION: &str = "vrl_position";

#[derive(Eq, PartialEq, Hash, Clone)]
struct RateKeyIdentifier {
    callsite: Identifier,
    rate_limit_key_values: RateLimitedSpanKeys,
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
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        {
            let span = ctx.span(id).expect("Span not found, this is a bug");
            let mut extensions = span.extensions_mut();

            if extensions.get_mut::<RateLimitedSpanKeys>().is_none() {
                let mut fields = RateLimitedSpanKeys::default();
                attrs.record(&mut fields);
                extensions.insert(fields);
            };
        }
        self.inner.on_new_span(attrs, id, ctx);
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

        let limit = limit_visitor.limit.unwrap_or(0);
        // If the event has a rate limit of 0 or an invalid rate limit, just pass through.
        // This has the same effect as allowing it through without the additional locking and state
        // initialization.
        if limit == 0 {
            return self.inner.on_event(event, ctx);
        }

        let rate_limit_key_values = {
            let scope = ctx
                .lookup_current()
                .into_iter()
                .flat_map(|span| span.scope().from_root());

            scope.fold(RateLimitedSpanKeys::default(), |mut keys, span| {
                let extensions = span.extensions();
                if let Some(span_keys) = extensions.get::<RateLimitedSpanKeys>() {
                    keys.merge(span_keys);
                }
                keys
            })
        };

        let id = RateKeyIdentifier {
            callsite: metadata.callsite(),
            rate_limit_key_values,
        };

        let mut state = self.events.entry(id).or_insert(State {
            start: Instant::now(),
            count: 0,
            limit,
            message: limit_visitor
                .message
                .unwrap_or_else(|| event.metadata().name().into()),
        });

        let prev = state.count;
        state.count += 1;

        //check if we are still rate limiting
        if state.start.elapsed().as_secs() < state.limit {
            // check and increment the current count
            // if 0: this is the first message, just pass it through
            // if 1: this is the first rate limited message
            // otherwise suppress it until the rate limit expires
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
        rate_limit: u64,
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
    limit: u64,
    message: String,
}

fn is_limited(metadata: &Metadata<'_>) -> bool {
    metadata
        .fields()
        .iter()
        .any(|f| f.name() == RATE_LIMIT_SECS_FIELD)
}

#[derive(PartialEq, Eq, Clone, Hash)]
enum TraceValue {
    String(String),
    Int(i64),
    Uint(u64),
    Bool(bool),
}

impl From<bool> for TraceValue {
    fn from(b: bool) -> Self {
        TraceValue::Bool(b)
    }
}

impl From<i64> for TraceValue {
    fn from(i: i64) -> Self {
        TraceValue::Int(i)
    }
}

impl From<u64> for TraceValue {
    fn from(u: u64) -> Self {
        TraceValue::Uint(u)
    }
}

impl From<String> for TraceValue {
    fn from(s: String) -> Self {
        TraceValue::String(s)
    }
}

/// RateLimitedSpanKeys records span keys that we use to rate limit callsites separately by. For
/// example, if a given trace callsite is called from two different components, then they will be
/// rate limited separately.
#[derive(Default, Eq, PartialEq, Hash, Clone)]
struct RateLimitedSpanKeys {
    component_id: Option<TraceValue>,
    vrl_line_number: Option<TraceValue>,
    vrl_position: Option<TraceValue>,
}

impl RateLimitedSpanKeys {
    fn record(&mut self, field: &Field, value: TraceValue) {
        match field.name() {
            COMPONENT_ID_FIELD => self.component_id = Some(value),
            VRL_LINE_NUMBER => self.vrl_line_number = Some(value),
            VRL_POSITION => self.vrl_position = Some(value),
            _ => {}
        }
    }

    fn merge(&mut self, other: &Self) {
        if let Some(component_id) = &other.component_id {
            self.component_id = Some(component_id.clone());
        }
        if let Some(vrl_line_number) = &other.vrl_line_number {
            self.vrl_line_number = Some(vrl_line_number.clone());
        }
        if let Some(vrl_position) = &other.vrl_position {
            self.vrl_position = Some(vrl_position.clone());
        }
    }
}

impl Visit for RateLimitedSpanKeys {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record(field, value.into());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record(field, value.into());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record(field, value.into());
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record(field, value.to_owned().into());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record(field, format!("{:?}", value).into());
    }
}

#[derive(Default)]
struct LimitVisitor {
    pub limit: Option<u64>,
    pub message: Option<String>,
}

impl Visit for LimitVisitor {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == RATE_LIMIT_SECS_FIELD {
            self.limit = Some(value);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == RATE_LIMIT_SECS_FIELD {
            self.limit = Some(u64::try_from(value).unwrap_or_default());
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
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use mock_instant::MockClock;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

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
                MockClock::advance(Duration::from_millis(100));
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
    fn rate_limit_by_span_key() {
        let events: Arc<Mutex<Vec<String>>> = Default::default();

        let recorder = RecordingLayer::new(Arc::clone(&events));
        let sub =
            tracing_subscriber::registry::Registry::default().with(RateLimitedLayer::new(recorder));
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                for key in &["foo", "bar"] {
                    for line_number in &[1, 2] {
                        let span =
                            info_span!("span", component_id = &key, vrl_line_number = &line_number);
                        let _enter = span.enter();
                        info!(
                            message =
                                format!("Hello {} on line_number {}!", key, line_number).as_str(),
                            internal_log_rate_secs = 1
                        );
                    }
                }
                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        assert_eq!(
            *events,
            vec![
                "Hello foo on line_number 1!",
                "Hello foo on line_number 2!",
                "Hello bar on line_number 1!",
                "Hello bar on line_number 2!",
                "Internal log [Hello foo on line_number 1!] is being rate limited.",
                "Internal log [Hello foo on line_number 2!] is being rate limited.",
                "Internal log [Hello bar on line_number 1!] is being rate limited.",
                "Internal log [Hello bar on line_number 2!] is being rate limited.",
                "Internal log [Hello foo on line_number 1!] has been rate limited 9 times.",
                "Hello foo on line_number 1!",
                "Internal log [Hello foo on line_number 2!] has been rate limited 9 times.",
                "Hello foo on line_number 2!",
                "Internal log [Hello bar on line_number 1!] has been rate limited 9 times.",
                "Hello bar on line_number 1!",
                "Internal log [Hello bar on line_number 2!] has been rate limited 9 times.",
                "Hello bar on line_number 2!",
                "Internal log [Hello foo on line_number 1!] is being rate limited.",
                "Internal log [Hello foo on line_number 2!] is being rate limited.",
                "Internal log [Hello bar on line_number 1!] is being rate limited.",
                "Internal log [Hello bar on line_number 2!] is being rate limited.",
                "Internal log [Hello foo on line_number 1!] has been rate limited 9 times.",
                "Hello foo on line_number 1!",
                "Internal log [Hello foo on line_number 2!] has been rate limited 9 times.",
                "Hello foo on line_number 2!",
                "Internal log [Hello bar on line_number 1!] has been rate limited 9 times.",
                "Hello bar on line_number 1!",
                "Internal log [Hello bar on line_number 2!] has been rate limited 9 times.",
                "Hello bar on line_number 2!",
            ]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<String>>()
        );
    }
}
