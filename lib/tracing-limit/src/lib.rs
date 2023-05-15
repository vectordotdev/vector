#![deny(warnings)]

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

const RATE_LIMIT_FIELD: &str = "internal_log_rate_limit";
const RATE_LIMIT_SECS_FIELD: &str = "internal_log_rate_secs";
const MESSAGE_FIELD: &str = "message";

// These fields will cause events to be independently rate limited by the values
// for these keys
const COMPONENT_ID_FIELD: &str = "component_id";
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
    internal_log_rate_limit: u64,
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
            internal_log_rate_limit: 10,
            inner: layer,
            _subscriber: std::marker::PhantomData,
        }
    }

    pub fn with_default_limit(mut self, internal_log_rate_limit: u64) -> Self {
        self.internal_log_rate_limit = internal_log_rate_limit;
        self
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
        // Visit the event, grabbing the limit status if one is defined. If we can't find a rate limit field, or the rate limit
        // is set as false, then we let it pass through untouched.
        let mut limit_visitor = LimitVisitor::default();
        event.record(&mut limit_visitor);

        let limit_exists = limit_visitor.limit.unwrap_or(false);
        if !limit_exists {
            return self.inner.on_event(event, ctx);
        }

        let limit = match limit_visitor.limit_secs {
            Some(limit_secs) => limit_secs, // override the cli limit
            None => self.internal_log_rate_limit,
        };

        // Visit all of the spans in the scope of this event, looking for specific fields that we use to differentiate
        // rate-limited events. This ensures that we don't rate limit an event's _callsite_, but the specific usage of a
        // callsite, since multiple copies of the same component could be running, etc.
        let rate_limit_key_values = {
            let mut keys = RateLimitedSpanKeys::default();
            event.record(&mut keys);

            ctx.lookup_current()
                .into_iter()
                .flat_map(|span| span.scope().from_root())
                .fold(keys, |mut keys, span| {
                    let extensions = span.extensions();
                    if let Some(span_keys) = extensions.get::<RateLimitedSpanKeys>() {
                        keys.merge(span_keys);
                    }
                    keys
                })
        };

        // Build the key to represent this event, given its span fields, and see if we're already rate limiting it. If
        // not, we'll initialize an entry for it.
        let metadata = event.metadata();
        let id = RateKeyIdentifier {
            callsite: metadata.callsite(),
            rate_limit_key_values,
        };

        let mut state = self.events.entry(id).or_insert_with(|| {
            let mut message_visitor = MessageVisitor::default();
            event.record(&mut message_visitor);

            let message = message_visitor
                .message
                .unwrap_or_else(|| metadata.name().into());

            State::new(message, limit)
        });

        // Update our suppressed state for this event, and see if we should still be suppressing it.
        //
        // When this is the first time seeing the event, we emit it like we normally would. The second time we see it in
        // the limit period, we emit a new event to indicate that the original event is being actively suppressed.
        // Otherwise, we don't emit anything.
        let previous_count = state.increment_count();
        if state.should_limit() {
            match previous_count {
                0 => self.inner.on_event(event, ctx),
                1 => {
                    let message = format!(
                        "Internal log [{}] is being suppressed to avoid flooding.",
                        state.message
                    );
                    self.create_event(&ctx, metadata, message, state.limit);
                }
                _ => {}
            }
        } else {
            // If we saw this event 3 or more times total, emit an event that indicates the total number of times we
            // suppressed the event in the limit period.
            if previous_count > 1 {
                let message = format!(
                    "Internal log [{}] has been suppressed {} times.",
                    state.message,
                    previous_count - 1
                );

                self.create_event(&ctx, metadata, message, state.limit);
            }

            // We're not suppressing anymore, so we also emit the current event as normal.. but we update our rate
            // limiting state since this is effectively equivalent to seeing the event again for the first time.
            self.inner.on_event(event, ctx);

            state.reset();
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

    #[inline]
    fn on_layer(&mut self, subscriber: &mut S) {
        self.inner.on_layer(subscriber);
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
                &fields.field(RATE_LIMIT_FIELD).unwrap(),
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

impl State {
    fn new(message: String, limit: u64) -> Self {
        Self {
            start: Instant::now(),
            count: 0,
            limit,
            message,
        }
    }

    fn reset(&mut self) {
        self.start = Instant::now();
        self.count = 1;
    }

    fn increment_count(&mut self) -> u64 {
        let prev = self.count;
        self.count += 1;
        prev
    }

    fn should_limit(&self) -> bool {
        self.start.elapsed().as_secs() < self.limit
    }
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
    vrl_position: Option<TraceValue>,
}

impl RateLimitedSpanKeys {
    fn record(&mut self, field: &Field, value: TraceValue) {
        match field.name() {
            COMPONENT_ID_FIELD => self.component_id = Some(value),
            VRL_POSITION => self.vrl_position = Some(value),
            _ => {}
        }
    }

    fn merge(&mut self, other: &Self) {
        if let Some(component_id) = &other.component_id {
            self.component_id = Some(component_id.clone());
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
    pub limit: Option<bool>,
    pub limit_secs: Option<u64>,
}

impl Visit for LimitVisitor {
    fn record_bool(&mut self, field: &Field, value: bool) {
        if field.name() == RATE_LIMIT_FIELD {
            self.limit = Some(value);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == RATE_LIMIT_SECS_FIELD {
            self.limit = Some(true); // limit if we have this field
            self.limit_secs = Some(u64::try_from(value).unwrap_or_default()); // override the cli passed limit
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == RATE_LIMIT_SECS_FIELD {
            self.limit = Some(true); // limit if we have this field
            self.limit_secs = Some(value); // override the cli passed limit
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {}
}

#[derive(Default)]
struct MessageVisitor {
    pub message: Option<String>,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if self.message.is_none() && field.name() == MESSAGE_FIELD {
            self.message = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if self.message.is_none() && field.name() == MESSAGE_FIELD {
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
            let mut visitor = MessageVisitor::default();
            event.record(&mut visitor);

            let mut events = self.events.lock().unwrap();
            events.push(visitor.message.unwrap_or_default());
        }
    }

    #[test]
    fn rate_limits() {
        let events: Arc<Mutex<Vec<String>>> = Default::default();

        let recorder = RecordingLayer::new(Arc::clone(&events));
        let sub = tracing_subscriber::registry::Registry::default()
            .with(RateLimitedLayer::new(recorder).with_default_limit(1));
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                info!(message = "Hello world!", internal_log_rate_limit = true);
                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        assert_eq!(
            *events,
            vec![
                "Hello world!",
                "Internal log [Hello world!] is being suppressed to avoid flooding.",
                "Internal log [Hello world!] has been suppressed 9 times.",
                "Hello world!",
                "Internal log [Hello world!] is being suppressed to avoid flooding.",
                "Internal log [Hello world!] has been suppressed 9 times.",
                "Hello world!",
            ]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<String>>()
        );
    }

    #[test]
    fn override_rate_limit_at_callsite() {
        let events: Arc<Mutex<Vec<String>>> = Default::default();

        let recorder = RecordingLayer::new(Arc::clone(&events));
        let sub = tracing_subscriber::registry::Registry::default()
            .with(RateLimitedLayer::new(recorder).with_default_limit(100));
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                info!(
                    message = "Hello world!",
                    internal_log_rate_limit = true,
                    internal_log_rate_secs = 1
                );
                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        assert_eq!(
            *events,
            vec![
                "Hello world!",
                "Internal log [Hello world!] is being suppressed to avoid flooding.",
                "Internal log [Hello world!] has been suppressed 9 times.",
                "Hello world!",
                "Internal log [Hello world!] is being suppressed to avoid flooding.",
                "Internal log [Hello world!] has been suppressed 9 times.",
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
        let sub = tracing_subscriber::registry::Registry::default()
            .with(RateLimitedLayer::new(recorder).with_default_limit(1));
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                for key in &["foo", "bar"] {
                    for line_number in &[1, 2] {
                        let span =
                            info_span!("span", component_id = &key, vrl_position = &line_number);
                        let _enter = span.enter();
                        info!(
                            message =
                                format!("Hello {} on line_number {}!", key, line_number).as_str(),
                            internal_log_rate_limit = true
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
                "Internal log [Hello foo on line_number 1!] is being suppressed to avoid flooding.",
                "Internal log [Hello foo on line_number 2!] is being suppressed to avoid flooding.",
                "Internal log [Hello bar on line_number 1!] is being suppressed to avoid flooding.",
                "Internal log [Hello bar on line_number 2!] is being suppressed to avoid flooding.",
                "Internal log [Hello foo on line_number 1!] has been suppressed 9 times.",
                "Hello foo on line_number 1!",
                "Internal log [Hello foo on line_number 2!] has been suppressed 9 times.",
                "Hello foo on line_number 2!",
                "Internal log [Hello bar on line_number 1!] has been suppressed 9 times.",
                "Hello bar on line_number 1!",
                "Internal log [Hello bar on line_number 2!] has been suppressed 9 times.",
                "Hello bar on line_number 2!",
                "Internal log [Hello foo on line_number 1!] is being suppressed to avoid flooding.",
                "Internal log [Hello foo on line_number 2!] is being suppressed to avoid flooding.",
                "Internal log [Hello bar on line_number 1!] is being suppressed to avoid flooding.",
                "Internal log [Hello bar on line_number 2!] is being suppressed to avoid flooding.",
                "Internal log [Hello foo on line_number 1!] has been suppressed 9 times.",
                "Hello foo on line_number 1!",
                "Internal log [Hello foo on line_number 2!] has been suppressed 9 times.",
                "Hello foo on line_number 2!",
                "Internal log [Hello bar on line_number 1!] has been suppressed 9 times.",
                "Hello bar on line_number 1!",
                "Internal log [Hello bar on line_number 2!] has been suppressed 9 times.",
                "Hello bar on line_number 2!",
            ]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<String>>()
        );
    }

    #[test]
    fn rate_limit_by_event_key() {
        let events: Arc<Mutex<Vec<String>>> = Default::default();

        let recorder = RecordingLayer::new(Arc::clone(&events));
        let sub = tracing_subscriber::registry::Registry::default()
            .with(RateLimitedLayer::new(recorder).with_default_limit(1));
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                for key in &["foo", "bar"] {
                    for line_number in &[1, 2] {
                        info!(
                            message =
                                format!("Hello {} on line_number {}!", key, line_number).as_str(),
                            internal_log_rate_limit = true,
                            component_id = &key,
                            vrl_position = &line_number
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
                "Internal log [Hello foo on line_number 1!] is being suppressed to avoid flooding.",
                "Internal log [Hello foo on line_number 2!] is being suppressed to avoid flooding.",
                "Internal log [Hello bar on line_number 1!] is being suppressed to avoid flooding.",
                "Internal log [Hello bar on line_number 2!] is being suppressed to avoid flooding.",
                "Internal log [Hello foo on line_number 1!] has been suppressed 9 times.",
                "Hello foo on line_number 1!",
                "Internal log [Hello foo on line_number 2!] has been suppressed 9 times.",
                "Hello foo on line_number 2!",
                "Internal log [Hello bar on line_number 1!] has been suppressed 9 times.",
                "Hello bar on line_number 1!",
                "Internal log [Hello bar on line_number 2!] has been suppressed 9 times.",
                "Hello bar on line_number 2!",
                "Internal log [Hello foo on line_number 1!] is being suppressed to avoid flooding.",
                "Internal log [Hello foo on line_number 2!] is being suppressed to avoid flooding.",
                "Internal log [Hello bar on line_number 1!] is being suppressed to avoid flooding.",
                "Internal log [Hello bar on line_number 2!] is being suppressed to avoid flooding.",
                "Internal log [Hello foo on line_number 1!] has been suppressed 9 times.",
                "Hello foo on line_number 1!",
                "Internal log [Hello foo on line_number 2!] has been suppressed 9 times.",
                "Hello foo on line_number 2!",
                "Internal log [Hello bar on line_number 1!] has been suppressed 9 times.",
                "Hello bar on line_number 1!",
                "Internal log [Hello bar on line_number 2!] has been suppressed 9 times.",
                "Hello bar on line_number 2!",
            ]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<String>>()
        );
    }
}
