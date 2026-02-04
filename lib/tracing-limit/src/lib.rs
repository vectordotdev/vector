#![deny(warnings)]
//! Rate limiting for tracing events.
//!
//! This crate provides a tracing-subscriber layer that rate limits log events to prevent
//! log flooding. Events are grouped by their callsite and contextual fields, with each
//! unique combination rate limited independently.
//!
//! # How it works
//!
//! Within each rate limit window (default 10 seconds):
//! - **1st occurrence**: Event is emitted normally
//! - **2nd occurrence**: Emits a "suppressing" warning
//! - **3rd+ occurrences**: Silent until window expires
//! - **After window**: Emits a summary of suppressed count, then next event normally
//!
//! # Rate limit grouping
//!
//! Events are rate limited independently based on a combination of:
//! - **Callsite**: The code location where the log statement appears
//! - **Contextual fields**: Any fields attached to the event or its parent spans
//!
//! ## How fields contribute to grouping
//!
//! **Only these fields create distinct rate limit groups:**
//! - `component_id` - Different components are rate limited independently
//!
//! **All other fields are ignored for grouping**, including:
//! - `fanout_id`, `input_id`, `output_id` - Not used for grouping to avoid resource/cost implications from high-cardinality tags
//! - `message` - The log message itself doesn't differentiate groups
//! - `internal_log_rate_limit` - Control field for enabling/disabling rate limiting
//! - `internal_log_rate_secs` - Control field for customizing the rate limit window
//! - Any custom fields you add
//!
//! ## Examples
//!
//! ```rust,ignore
//! // Example 1: Different component_id values create separate rate limit groups
//! info!(component_id = "transform_1", "Processing event");  // Group A
//! info!(component_id = "transform_2", "Processing event");  // Group B
//! // Even though the message is identical, these are rate limited independently
//!
//! // Example 2: Only component_id matters for grouping
//! info!(component_id = "router", fanout_id = "output_1", "Routing event");  // Group C
//! info!(component_id = "router", fanout_id = "output_2", "Routing event");  // Group C (same group!)
//! info!(component_id = "router", fanout_id = "output_1", "Routing event");  // Group C (same group!)
//! info!(component_id = "router", fanout_id = "output_1", input_id = "kafka", "Routing event");  // Group C (same!)
//! // All of these share the same group because they have the same component_id
//! // The fanout_id and input_id fields are ignored to avoid resource/cost implications
//!
//! // Example 3: Span fields contribute to grouping
//! let span = info_span!("process", component_id = "transform_1");
//! let _enter = span.enter();
//! info!("Processing event");  // Group E: callsite + component_id from span
//! drop(_enter);
//!
//! let span = info_span!("process", component_id = "transform_2");
//! let _enter = span.enter();
//! info!("Processing event");  // Group F: same callsite but different component_id
//!
//! // Example 4: Nested spans - child span fields take precedence
//! let outer = info_span!("outer", component_id = "parent");
//! let _outer_guard = outer.enter();
//! let inner = info_span!("inner", component_id = "child");
//! let _inner_guard = inner.enter();
//! info!("Nested event");  // Grouped by component_id = "child"
//!
//! // Example 5: Same callsite with no fields = single rate limit group
//! info!("Simple message");  // Group G
//! info!("Simple message");  // Group G
//! info!("Simple message");  // Group G
//!
//! // Example 6: Custom fields are ignored for grouping
//! info!(component_id = "source", input_id = "in_1", "Received data");  // Group H
//! info!(component_id = "source", input_id = "in_2", "Received data");  // Group H (same group!)
//! // The input_id field is ignored - only component_id matters
//!
//! // Example 7: Disabling rate limiting for specific logs
//! // Rate limiting is ON by default - explicitly disable for important logs
//! warn!(
//!     component_id = "critical_component",
//!     message = "Fatal error occurred",
//!     internal_log_rate_limit = false
//! );
//! // This event will NEVER be rate limited, regardless of how often it fires
//!
//! // Example 8: Custom rate limit window for specific events
//! info!(
//!     component_id = "noisy_component",
//!     message = "Frequent status update",
//!     internal_log_rate_secs = 60  // Only log once per minute
//! );
//! // Override the default window for this specific log
//! ```
//!
//! This ensures logs from different components are rate limited independently,
//! while avoiding resource/cost implications from high-cardinality tags.

use std::fmt;

use dashmap::DashMap;
use tracing_core::{
    Event, Metadata, Subscriber,
    callsite::Identifier,
    field::{Field, Value, Visit, display},
    span,
    subscriber::Interest,
};
use tracing_subscriber::layer::{Context, Layer};

#[cfg(test)]
#[macro_use]
extern crate tracing;

#[cfg(not(test))]
use std::time::Instant;

#[cfg(test)]
use mock_instant::global::Instant;

const RATE_LIMIT_FIELD: &str = "internal_log_rate_limit";
const RATE_LIMIT_SECS_FIELD: &str = "internal_log_rate_secs";
const MESSAGE_FIELD: &str = "message";

// These fields will cause events to be independently rate limited by the values
// for these keys
const COMPONENT_ID_FIELD: &str = "component_id";

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

    /// Sets the default rate limit window in seconds.
    ///
    /// This controls how long logs are suppressed before they can be emitted again.
    /// Within each window:
    /// - 1st occurrence: Emitted normally
    /// - 2nd occurrence: Shows "suppressing" warning
    /// - 3rd+ occurrences: Silent until window expires
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
        // Visit the event, grabbing the limit status if one is defined. Rate limiting is ON by default
        // unless explicitly disabled by setting `internal_log_rate_limit = false`.
        let mut limit_visitor = LimitVisitor::default();
        event.record(&mut limit_visitor);

        let limit_exists = limit_visitor.limit.unwrap_or(true);
        if !limit_exists {
            return self.inner.on_event(event, ctx);
        }

        let limit = match limit_visitor.limit_secs {
            Some(limit_secs) => limit_secs, // override the cli limit
            None => self.internal_log_rate_limit,
        };

        // Build a composite key from event fields and span context to determine the rate limit group.
        // This multi-step process ensures we capture all relevant contextual information:
        //
        // 1. Start with event-level fields (e.g., fields directly on the log macro call)
        // 2. Walk up the span hierarchy from root to current span
        // 3. Merge in fields from each span, with child spans taking precedence
        //
        // This means an event's rate limit group is determined by the combination of:
        // - Its callsite (handled separately via RateKeyIdentifier)
        // - All contextual fields from both the event and its span ancestry
        //
        // Example: The same `info!("msg")` callsite in different component contexts becomes
        // distinct rate limit groups, allowing fine-grained control over log flooding.
        let rate_limit_key_values = {
            let mut keys = RateLimitedSpanKeys::default();
            // Capture fields directly on this event
            event.record(&mut keys);

            // Walk span hierarchy and merge in contextual fields
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
        } else if let Some(rate_limit_field) = fields.field(RATE_LIMIT_FIELD) {
            let values = [(&rate_limit_field, Some(&rate_limit as &dyn Value))];

            let valueset = fields.value_set(&values);
            let event = Event::new(metadata, &valueset);
            self.inner.on_event(&event, ctx.clone());
        } else {
            // If the event metadata has neither a "message" nor "internal_log_rate_limit" field,
            // we cannot create a proper synthetic event. This can happen with custom debug events
            // that have their own field structure. In this case, we simply skip emitting the
            // rate limit notification rather than panicking.
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

#[cfg(test)]
impl fmt::Display for TraceValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TraceValue::String(s) => write!(f, "{}", s),
            TraceValue::Int(i) => write!(f, "{}", i),
            TraceValue::Uint(u) => write!(f, "{}", u),
            TraceValue::Bool(b) => write!(f, "{}", b),
        }
    }
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

/// RateLimitedSpanKeys records span and event fields that differentiate rate limit groups.
///
/// This struct is used to build a composite key that uniquely identifies a rate limit bucket.
/// Events with different field values will be rate limited independently, even if they come
/// from the same callsite.
///
/// ## Field categories:
///
/// **Tracked fields** (only these create distinct rate limit groups):
/// - `component_id` - Different components are rate limited independently
///
/// **Ignored fields**: All other fields are ignored for grouping purposes. This avoids resource/cost implications from high-cardinality tags.
/// ```
#[derive(Default, Eq, PartialEq, Hash, Clone)]
struct RateLimitedSpanKeys {
    component_id: Option<TraceValue>,
}

impl RateLimitedSpanKeys {
    fn record(&mut self, field: &Field, value: TraceValue) {
        if field.name() == COMPONENT_ID_FIELD {
            self.component_id = Some(value);
        }
    }

    fn merge(&mut self, other: &Self) {
        if let Some(component_id) = &other.component_id {
            self.component_id = Some(component_id.clone());
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
        self.record(field, format!("{value:?}").into());
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
            self.message = Some(format!("{value:?}"));
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use mock_instant::global::MockClock;
    use serial_test::serial;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedEvent {
        message: String,
        fields: BTreeMap<String, String>,
    }

    impl RecordedEvent {
        fn new(message: impl Into<String>) -> Self {
            Self {
                message: message.into(),
                fields: BTreeMap::new(),
            }
        }

        fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
            self.fields.insert(key.into(), value.into());
            self
        }
    }

    /// Macro to create RecordedEvent with optional fields
    /// Usage:
    /// - `event!("message")` - just message
    /// - `event!("message", key1: "value1")` - message with one field
    /// - `event!("message", key1: "value1", key2: "value2")` - message with multiple fields
    macro_rules! event {
        ($msg:expr) => {
            RecordedEvent::new($msg)
        };
        ($msg:expr, $($key:ident: $value:expr),+ $(,)?) => {
            RecordedEvent::new($msg)
                $(.with_field(stringify!($key), $value))+
        };
    }

    #[derive(Default)]
    struct AllFieldsVisitor {
        fields: BTreeMap<String, String>,
    }

    impl Visit for AllFieldsVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_i64(&mut self, field: &Field, value: i64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_u64(&mut self, field: &Field, value: u64) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }

        fn record_bool(&mut self, field: &Field, value: bool) {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }

    impl AllFieldsVisitor {
        fn into_event(self) -> RecordedEvent {
            let message = self
                .fields
                .get("message")
                .cloned()
                .unwrap_or_else(|| String::from(""));

            let mut fields = BTreeMap::new();
            for (key, value) in self.fields {
                if key != "message"
                    && key != "internal_log_rate_limit"
                    && key != "internal_log_rate_secs"
                {
                    fields.insert(key, value);
                }
            }

            RecordedEvent { message, fields }
        }
    }

    #[derive(Default)]
    struct RecordingLayer<S> {
        events: Arc<Mutex<Vec<RecordedEvent>>>,

        _subscriber: std::marker::PhantomData<S>,
    }

    impl<S> RecordingLayer<S> {
        fn new(events: Arc<Mutex<Vec<RecordedEvent>>>) -> Self {
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

        fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
            let mut visitor = AllFieldsVisitor::default();
            event.record(&mut visitor);

            // Also capture fields from span context
            if let Some(span) = ctx.lookup_current() {
                for span_ref in span.scope().from_root() {
                    let extensions = span_ref.extensions();
                    if let Some(span_keys) = extensions.get::<RateLimitedSpanKeys>() {
                        // Add component_id
                        if let Some(TraceValue::String(ref s)) = span_keys.component_id {
                            visitor.fields.insert("component_id".to_string(), s.clone());
                        }
                    }
                }
            }

            let mut events = self.events.lock().unwrap();
            events.push(visitor.into_event());
        }
    }

    /// Helper function to set up a test with a rate-limited subscriber.
    /// Returns the events Arc for asserting on collected events.
    fn setup_test(
        default_limit: u64,
    ) -> (
        Arc<Mutex<Vec<RecordedEvent>>>,
        impl Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    ) {
        let events: Arc<Mutex<Vec<RecordedEvent>>> = Default::default();
        let recorder = RecordingLayer::new(Arc::clone(&events));
        let sub = tracing_subscriber::registry::Registry::default()
            .with(RateLimitedLayer::new(recorder).with_default_limit(default_limit));
        (events, sub)
    }

    #[test]
    #[serial]
    fn rate_limits() {
        let (events, sub) = setup_test(1);
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                info!(message = "Hello world!");
                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        assert_eq!(
            *events,
            vec![
                event!("Hello world!"),
                event!("Internal log [Hello world!] is being suppressed to avoid flooding."),
                event!("Internal log [Hello world!] has been suppressed 9 times."),
                event!("Hello world!"),
                event!("Internal log [Hello world!] is being suppressed to avoid flooding."),
                event!("Internal log [Hello world!] has been suppressed 9 times."),
                event!("Hello world!"),
            ]
        );
    }

    #[test]
    #[serial]
    fn override_rate_limit_at_callsite() {
        let (events, sub) = setup_test(100);
        tracing::subscriber::with_default(sub, || {
            for _ in 0..31 {
                info!(message = "Hello world!", internal_log_rate_secs = 2);
                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        // With a 2-second window and 100ms advances, we get:
        // - Event every 20 iterations (2000ms / 100ms = 20)
        // - First window: iteration 0-19 (suppressed 19 times after first 2)
        // - Second window: iteration 20-39 (but we only go to 30)
        assert_eq!(
            *events,
            vec![
                event!("Hello world!"),
                event!("Internal log [Hello world!] is being suppressed to avoid flooding."),
                event!("Internal log [Hello world!] has been suppressed 19 times."),
                event!("Hello world!"),
                event!("Internal log [Hello world!] is being suppressed to avoid flooding."),
            ]
        );
    }

    #[test]
    #[serial]
    fn rate_limit_by_event_key() {
        let (events, sub) = setup_test(1);
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                for key in &["foo", "bar"] {
                    info!(
                        message = format!("Hello {key}!").as_str(),
                        component_id = &key
                    );
                }
                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        // Events with different component_id values create separate rate limit groups
        assert_eq!(
            *events,
            vec![
                event!("Hello foo!", component_id: "foo"),
                event!("Hello bar!", component_id: "bar"),
                event!("Internal log [Hello foo!] is being suppressed to avoid flooding."),
                event!("Internal log [Hello bar!] is being suppressed to avoid flooding."),
                event!("Internal log [Hello foo!] has been suppressed 9 times."),
                event!("Hello foo!", component_id: "foo"),
                event!("Internal log [Hello bar!] has been suppressed 9 times."),
                event!("Hello bar!", component_id: "bar"),
                event!("Internal log [Hello foo!] is being suppressed to avoid flooding."),
                event!("Internal log [Hello bar!] is being suppressed to avoid flooding."),
                event!("Internal log [Hello foo!] has been suppressed 9 times."),
                event!("Hello foo!", component_id: "foo"),
                event!("Internal log [Hello bar!] has been suppressed 9 times."),
                event!("Hello bar!", component_id: "bar"),
            ]
        );
    }

    #[test]
    #[serial]
    fn disabled_rate_limit() {
        let (events, sub) = setup_test(1);
        tracing::subscriber::with_default(sub, || {
            for _ in 0..21 {
                info!(message = "Hello world!", internal_log_rate_limit = false);
                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        // All 21 events should be emitted since rate limiting is disabled
        assert_eq!(events.len(), 21);
        assert!(events.iter().all(|e| e == &event!("Hello world!")));
    }

    #[test]
    #[serial]
    fn rate_limit_ignores_non_special_fields() {
        let (events, sub) = setup_test(1);
        tracing::subscriber::with_default(sub, || {
            for i in 0..21 {
                // Call the SAME info! macro multiple times per iteration with varying fanout_id
                // to verify that fanout_id doesn't create separate rate limit groups
                for _ in 0..3 {
                    let fanout = if i % 2 == 0 { "output_1" } else { "output_2" };
                    info!(
                        message = "Routing event",
                        component_id = "router",
                        fanout_id = fanout
                    );
                }
                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        // All events share the same rate limit group (same callsite + component_id)
        // First event emits normally, second shows suppression, third and beyond are silent
        // until the window expires
        assert_eq!(
            *events,
            vec![
                // First iteration - first emits, second shows suppression, 3rd+ silent
                event!("Routing event", component_id: "router", fanout_id: "output_1"),
                event!("Internal log [Routing event] is being suppressed to avoid flooding."),
                // After rate limit window (1 sec) - summary shows suppressions
                event!("Internal log [Routing event] has been suppressed 29 times."),
                event!("Routing event", component_id: "router", fanout_id: "output_1"),
                event!("Internal log [Routing event] is being suppressed to avoid flooding."),
                event!("Internal log [Routing event] has been suppressed 29 times."),
                event!("Routing event", component_id: "router", fanout_id: "output_1"),
                event!("Internal log [Routing event] is being suppressed to avoid flooding."),
            ]
        );
    }

    #[test]
    #[serial]
    fn nested_spans_child_takes_precedence() {
        let (events, sub) = setup_test(1);
        tracing::subscriber::with_default(sub, || {
            // Create nested spans where child overrides parent's component_id
            let outer = info_span!("outer", component_id = "parent");
            let _outer_guard = outer.enter();

            for _ in 0..21 {
                // Inner span with different component_id should take precedence
                let inner = info_span!("inner", component_id = "child");
                let _inner_guard = inner.enter();
                info!(message = "Nested event");
                drop(_inner_guard);

                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        // All events should be grouped by component_id = "child" (from inner span)
        // not "parent" (from outer span), demonstrating child precedence
        assert_eq!(
            *events,
            vec![
                event!("Nested event", component_id: "child"),
                event!("Internal log [Nested event] is being suppressed to avoid flooding.", component_id: "child"),
                event!("Internal log [Nested event] has been suppressed 9 times.", component_id: "child"),
                event!("Nested event", component_id: "child"),
                event!("Internal log [Nested event] is being suppressed to avoid flooding.", component_id: "child"),
                event!("Internal log [Nested event] has been suppressed 9 times.", component_id: "child"),
                event!("Nested event", component_id: "child"),
            ]
        );
    }

    #[test]
    #[serial]
    fn nested_spans_ignores_untracked_fields() {
        let (events, sub) = setup_test(1);
        tracing::subscriber::with_default(sub, || {
            // Parent has component_id, child has some_field - only component_id is tracked
            let outer = info_span!("outer", component_id = "transform");
            let _outer_guard = outer.enter();

            for _ in 0..21 {
                let inner = info_span!("inner", some_field = "value");
                let _inner_guard = inner.enter();
                info!(message = "Event message");
                drop(_inner_guard);

                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        // Events should have component_id from parent, some_field from child is ignored for grouping
        // All events are in the same rate limit group
        assert_eq!(
            *events,
            vec![
                event!("Event message", component_id: "transform"),
                event!(
                    "Internal log [Event message] is being suppressed to avoid flooding.",
                    component_id: "transform"
                ),
                event!(
                    "Internal log [Event message] has been suppressed 9 times.",
                    component_id: "transform"
                ),
                event!("Event message", component_id: "transform"),
                event!(
                    "Internal log [Event message] is being suppressed to avoid flooding.",
                    component_id: "transform"
                ),
                event!(
                    "Internal log [Event message] has been suppressed 9 times.",
                    component_id: "transform"
                ),
                event!("Event message", component_id: "transform"),
            ]
        );
    }

    #[test]
    #[serial]
    fn rate_limit_same_message_different_component() {
        let (events, sub) = setup_test(1);
        tracing::subscriber::with_default(sub, || {
            // Use a loop with the SAME callsite to demonstrate that identical messages
            // with different component_ids create separate rate limit groups
            for component in &["foo", "foo", "bar"] {
                info!(message = "Hello!", component_id = component);
                MockClock::advance(Duration::from_millis(100));
            }
        });

        let events = events.lock().unwrap();

        // The first "foo" event is emitted normally (count=0)
        // The second "foo" event triggers suppression warning (count=1)
        // The "bar" event is emitted normally (count=0 for its group)
        // This proves that even with identical message text, different component_ids
        // create separate rate limit groups
        assert_eq!(
            *events,
            vec![
                event!("Hello!", component_id: "foo"),
                event!("Internal log [Hello!] is being suppressed to avoid flooding."),
                event!("Hello!", component_id: "bar"),
            ]
        );
    }

    #[test]
    #[serial]
    fn events_with_custom_fields_no_message_dont_panic() {
        // Verify events without "message" or "internal_log_rate_limit" fields don't panic
        // when rate limiting skips suppression notifications.
        let (events, sub) = setup_test(1);
        tracing::subscriber::with_default(sub, || {
            // Use closure to ensure all events share the same callsite
            let emit_event = || {
                debug!(component_id = "test_component", utilization = 0.85);
            };

            // First window: emit 5 events, only the first one should be logged
            for _ in 0..5 {
                emit_event();
                MockClock::advance(Duration::from_millis(100));
            }

            // Advance to the next window
            MockClock::advance(Duration::from_millis(1000));

            // Second window: this event should be logged
            emit_event();
        });

        let events = events.lock().unwrap();

        // First event from window 1, first event from window 2
        // Suppression notifications are skipped (no message field)
        assert_eq!(
            *events,
            vec![
                event!("", component_id: "test_component", utilization: "0.85"),
                event!("", component_id: "test_component", utilization: "0.85"),
            ]
        );
    }
}
