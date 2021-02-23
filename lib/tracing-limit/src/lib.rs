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

const RATE_LIMIT_FIELD: &str = "internal_log_rate_secs";
const MESSAGE_FIELD: &str = "message";
const DEFAULT_LIMIT: u64 = 5;

pub struct RateLimitedLayer<S, L>
where
    L: Layer<S> + Sized,
    S: Subscriber,
{
    events: RwLock<HashMap<Identifier, State>>,
    callsite_store: RwLock<HashMap<Identifier, &'static Metadata<'static>>>,
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
            events: RwLock::<HashMap<Identifier, State>>::default(),
            callsite_store: RwLock::<HashMap<Identifier, &'static Metadata<'static>>>::default(),
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
            .any(|f| f.name() == RATE_LIMIT_FIELD)
        {
            let id = metadata.callsite();
            let mut events = self.events.write().expect("lock poisoned!");

            let state = State {
                start: None,
                count: AtomicUsize::new(0),
                limit: DEFAULT_LIMIT,
                message: String::new(),
            };

            events.insert(id.clone(), state);
            drop(events);

            let mut callsite_store = self.callsite_store.write().unwrap();
            callsite_store.insert(id, metadata);
            drop(callsite_store);

            Interest::sometimes()
        } else {
            inner
        }
    }

    fn enabled(&self, metadata: &Metadata<'_>, ctx: Context<'_, S>) -> bool {
        if !self.inner.enabled(metadata, ctx.clone()) {
            // if wrapped layer doesn't care about the event, don't bother rate limiting
            return false;
        }

        if ctx.enabled(metadata) {
            let events = self.events.read().expect("lock poisoned!");
            let id = metadata.callsite();

            if events.contains_key(&id) {
                // check if the event exists within the map, if it does
                // that means we are currently rate limiting it.
                if let Some(state) = events.get(&id) {
                    let start = state.start.unwrap_or_else(Instant::now);

                    if start.elapsed().as_secs() < state.limit {
                        let prev = state.count.fetch_add(1, Ordering::Relaxed);
                        match prev {
                            0 => return true,
                            1 => {
                                let message = format!(
                                    "Internal log [{}] is being rate limited.",
                                    state.message
                                );

                                self.create_event(&id, &ctx, message, state.limit);
                            }
                            _ => (),
                        }
                    } else {
                        drop(events);

                        let mut events = self.events.write().expect("lock poisoned!");

                        if let Some(state) = events.remove(&id) {
                            let count = state.count.load(Ordering::Relaxed);
                            if count > 1 {
                                let message = format!(
                                    "Internal log [{}] has been rate limited {} times.",
                                    state.message,
                                    count - 1
                                );

                                self.create_event(&id, &ctx, message, state.limit);
                            }
                        }
                    }

                    return false;
                }
            }
        }

        true
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

    #[inline]
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if is_limited(event.metadata()) {
            let mut limit_visitor = LimitVisitor::default();
            event.record(&mut limit_visitor);

            if let Some(limit) = limit_visitor.limit {
                let start = Instant::now();

                let id = event.metadata().callsite();

                let mut events = self.events.write().expect("lock poisoned!");

                let state = State {
                    start: Some(start),
                    count: AtomicUsize::new(1),
                    limit: limit as u64,
                    message: limit_visitor
                        .message
                        .unwrap_or_else(|| event.metadata().name().into()),
                };

                events.insert(id, state);
            }
        }
        self.inner.on_event(event, ctx);
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
    fn create_event(&self, id: &Identifier, ctx: &Context<S>, message: String, rate_limit: u64) {
        let store = self.callsite_store.read().unwrap();
        let metadata = store.get(id).unwrap();

        let fields = metadata.fields();

        let message = display(message);

        if let Some(message_field) = fields.field("message") {
            let values = [(&message_field, Some(&message as &dyn Value))];

            let valueset = fields.value_set(&values);
            let event = Event::new(metadata, &valueset);
            self.inner.on_event(&event, ctx.clone());
            drop(store);
        } else {
            let values = [(
                &fields.field(RATE_LIMIT_FIELD).unwrap(),
                Some(&rate_limit as &dyn Value),
            )];

            let valueset = fields.value_set(&values);
            let event = Event::new(metadata, &valueset);
            self.inner.on_event(&event, ctx.clone());
            drop(store);
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
        .any(|f| f.name() == RATE_LIMIT_FIELD)
}

#[derive(Default)]
struct LimitVisitor {
    pub limit: Option<usize>,
    pub message: Option<String>,
}

impl Visit for LimitVisitor {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == RATE_LIMIT_FIELD {
            self.limit = Some(value as usize);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == RATE_LIMIT_FIELD {
            self.limit = Some(value as usize);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == MESSAGE_FIELD {
            self.message = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {}
}
