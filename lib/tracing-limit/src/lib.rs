use std::fmt;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        RwLock,
    },
    time::Instant,
};
use tracing_core::{
    callsite::Identifier,
    field::{display, Field, Value, Visit},
    span,
    subscriber::Interest,
    Event, Metadata, Subscriber,
};
use tracing_subscriber::layer::{Context, Layer};

const RATE_LIMIT_SECS_FIELD: &str = "internal_log_rate_secs";
const RATE_LIMIT_KEY_FIELD: &str = "internal_log_rate_key";
const MESSAGE_FIELD: &str = "message";

#[derive(Eq, PartialEq, Hash)]
struct RateKeyIdentifier(Identifier, Option<String>);

pub struct RateLimitedLayer<S, L>
where
    L: Layer<S> + Sized,
    S: Subscriber,
{
    events: RwLock<HashMap<RateKeyIdentifier, State>>,
    inner: L,

    // TODO is this right?
    _subscriber: std::marker::PhantomData<S>,
}

impl<S, L> RateLimitedLayer<S, L>
where
    L: Layer<S> + Sized,
    S: Subscriber,
{
    pub fn new(layer: L) -> Self {
        RateLimitedLayer {
            events: RwLock::<HashMap<RateKeyIdentifier, State>>::default(),
            inner: layer,
            _subscriber: std::marker::PhantomData,
        }
    }
}

impl<S, L> Layer<S> for RateLimitedLayer<S, L>
where
    L: Layer<S>,
    S: Subscriber,
{
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        let inner = self.inner.register_callsite(metadata);
        if inner.is_never() {
            // if wrapped layer doesn't care about the event, don't bother rate limiting
            return inner;
        }

        if metadata
            .fields()
            .iter()
            .any(|f| f.name() == RATE_LIMIT_SECS_FIELD)
        {
            Interest::sometimes()
        } else {
            inner
        }
    }

    #[inline]
    fn enabled(&self, metadata: &Metadata<'_>, ctx: Context<'_, S>) -> bool {
        self.inner.enabled(metadata, ctx)
    }

    #[inline]
    fn new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        self.inner.new_span(attrs, id, ctx);
    }

    #[inline]
    fn on_record(&self, span: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        self.inner.on_record(span, values, ctx);
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

        let id = RateKeyIdentifier(metadata.callsite(), limit_visitor.key.clone());

        let events = self.events.read().expect("lock poisoned!");

        // check if the event exists within the map, if it does
        // that means we are currently rate limiting it.
        if let Some(state) = events.get(&id) {
            let start = state.start.unwrap_or_else(Instant::now);

            // check if we are still rate limiting
            if start.elapsed().as_secs() < state.limit {
                let prev = state.count.fetch_add(1, Ordering::Relaxed);
                match prev {
                    1 => {
                        // output first rate limited log
                        let message = match limit_visitor.key {
                            None => {
                                format!("Internal log [{}] is being rate limited.", state.message)
                            }
                            Some(key) => format!(
                                "Internal log [{} internal_log_rate_key={}].",
                                state.message, key,
                            ),
                        };

                        self.create_event(&ctx, metadata, message, state.limit);
                    }
                    // swallow the rest until a log comes in after the internal_log_rate_secs
                    // interval
                    _ => (),
                }
            } else {
                // done rate limiting
                drop(events);

                let mut events = self.events.write().expect("lock poisoned!");
                if let Some(state) = events.remove(&id) {
                    let count = state.count.load(Ordering::Relaxed);

                    // avoid outputting a message if the event wasn't rate limited
                    if count > 1 {
                        let message = format!(
                            "Internal log [{}] has been rate limited {} times.",
                            state.message,
                            count - 1
                        );

                        self.create_event(&ctx, metadata, message, state.limit);
                    }
                }
            }
        } else {
            drop(events);
            if let Some(limit) = limit_visitor.limit {
                let mut events = self.events.write().expect("lock poisoned!");

                let state = State {
                    start: Some(Instant::now()),
                    count: AtomicUsize::new(1),
                    limit: limit as u64,
                    message: limit_visitor
                        .message
                        .unwrap_or_else(|| event.metadata().name().into()),
                };

                events.insert(id, state);
            }

            self.inner.on_event(event, ctx);
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
    start: Option<Instant>,
    count: AtomicUsize,
    limit: u64,
    message: String,
}

fn is_limited(metadata: &Metadata<'_>) -> bool {
    metadata
        .fields()
        .iter()
        .any(|f| f.name() == RATE_LIMIT_SECS_FIELD)
}

#[derive(Default)]
struct LimitVisitor {
    pub limit: Option<usize>,
    pub message: Option<String>,
    pub key: Option<String>,
}

impl Visit for LimitVisitor {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == RATE_LIMIT_SECS_FIELD {
            self.limit = Some(value as usize);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == RATE_LIMIT_SECS_FIELD {
            self.limit = Some(value as usize);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            MESSAGE_FIELD => self.message = Some(value.to_string()),
            RATE_LIMIT_KEY_FIELD => self.key = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {}
}
