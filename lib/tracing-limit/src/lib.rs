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
    subscriber::Interest,
    Event, Metadata, Subscriber,
};
use tracing_subscriber::layer::{Context, Layer};

const RATE_LIMIT_FIELD: &str = "rate_limit_secs";
const MESSAGE_FIELD: &str = "message";
const DEFAULT_LIMIT: u64 = 5;

#[derive(Debug, Default)]
pub struct Limit {
    events: RwLock<HashMap<Identifier, State>>,
    callsite_store: RwLock<HashMap<Identifier, &'static Metadata<'static>>>,
}

#[derive(Debug)]
struct State {
    start: Option<Instant>,
    count: AtomicUsize,
    limit: u64,
    message: String,
}

impl Limit {
    fn create_event<S: Subscriber>(&self, id: &Identifier, ctx: &Context<S>, message: String) {
        let store = self.callsite_store.read().unwrap();
        let metadata = store.get(id).unwrap();

        let fields = metadata.fields();

        let message = display(message);

        if let Some(message_field) = fields.field("message") {
            let values = [
                (&message_field, Some(&message as &dyn Value)),
                (
                    &fields.field(RATE_LIMIT_FIELD).unwrap(),
                    Some(&5 as &dyn Value),
                ),
            ];

            let valueset = fields.value_set(&values);
            let event = Event::new(metadata, &valueset);
            ctx.event(&event);
            drop(store);
        } else {
            let values = [(
                &fields.field(RATE_LIMIT_FIELD).unwrap(),
                Some(&5 as &dyn Value),
            )];

            let valueset = fields.value_set(&values);
            let event = Event::new(metadata, &valueset);
            ctx.event(&event);
            drop(store);
        }
    }
}

impl<S> Layer<S> for Limit
where
    S: Subscriber,
    Self: 'static,
{
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
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
            Interest::always()
        }
    }

    fn enabled(&self, metadata: &Metadata, ctx: Context<S>) -> bool {
        if ctx.enabled(metadata) {
            let events = self.events.read().expect("lock poisoned!");
            let id = metadata.callsite();

            if events.contains_key(&id) {
                // check if the event exists within the map, if it does
                // that means we are currently rate limiting it.
                if let Some(state) = events.get(&id) {
                    let start = state.start.unwrap_or_else(Instant::now);

                    if start.elapsed().as_secs() < state.limit {
                        if state.count.load(Ordering::Acquire) == 1 {
                            let message = format!("{:?} is being rate limited.", state.message);

                            self.create_event(&id, &ctx, message);
                        }

                        let prev = state.count.fetch_add(1, Ordering::Relaxed);

                        if prev == 0 {
                            return true;
                        }
                    } else {
                        drop(events);

                        let mut events = self.events.write().expect("lock poisoned!");

                        if let Some(state) = events.remove(&id) {
                            let message = format!(
                                "{:?} {:?} events were rate limited.",
                                state.count, state.message
                            );

                            self.create_event(&id, &ctx, message);
                        }
                    }

                    return false;
                }
            }
        }

        true
    }

    fn on_event(&self, event: &Event, _ctx: Context<S>) {
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
    }
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
