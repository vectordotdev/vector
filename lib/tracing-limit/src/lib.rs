use std::fmt;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        RwLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tracing_core::{
    callsite::Identifier,
    field::{Field, Value, Visit},
    subscriber::Interest,
    Event, Metadata, Subscriber,
};
use tracing_subscriber::layer::{Context, Layer};

const RATE_LIMIT_FIELD: &'static str = "rate_limit_secs";
const MESSAGE_FIELD: &'static str = "message";

#[derive(Debug, Default)]
pub struct Limit {
    events: RwLock<HashMap<Identifier, State>>,
    callsite_store: RwLock<HashMap<Identifier, &'static Metadata<'static>>>,
}

#[derive(Debug)]
struct State {
    start: usize,
    count: AtomicUsize,
    limit: usize,
    message: Option<String>,
}

impl Limit {
    fn create_event<S: Subscriber>(&self, id: &Identifier, ctx: &Context<S>, message: String) {
        let store = self.callsite_store.read().unwrap();
        let metadata = store.get(id).unwrap();

        let fields = metadata.fields();

        let message = tracing_core::field::display(message);

        let values = [
            (
                &fields.field("message").unwrap(),
                Some(&message as &dyn Value),
            ),
            (
                &fields.field(RATE_LIMIT_FIELD).unwrap(),
                Some(&5 as &dyn Value),
            ),
        ];
        let valueset = fields.value_set(&values);

        let event = Event::new(metadata, &valueset);
        ctx.event(&event);
        drop(store);
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
            let start = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as usize;

            let id = metadata.callsite();

            let mut events = self.events.write().expect("lock poisoned!");

            let state = State {
                start,
                count: AtomicUsize::new(0),
                limit: 5,
                message: None,
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
        if is_limited(metadata) {
            let id = metadata.callsite();
            let events = self.events.read().expect("lock poisoned!");

            // check if the event exists within the map, if it does
            // that means we are currently rate limiting it.
            if let Some(state) = events.get(&id) {
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as usize;

                if ts - state.start < state.limit {
                    if state.count.load(Ordering::SeqCst) == 1 {
                        let message = if let Some(event_message) = &state.message {
                            format!("{:?} is being rate limited.", event_message)
                        } else {
                            format!("unknown message is being rate limited.")
                        };

                        self.create_event(&id, &ctx, message);
                    }

                    let prev = state.count.fetch_add(1, Ordering::SeqCst);

                    if prev == 0 {
                        return true;
                    }
                } else {
                    drop(events);

                    let mut events = self.events.write().expect("lock poisoned!");

                    if let Some(state) = events.remove(&id) {
                        let message = if let Some(event_message) = &state.message {
                            format!(
                                "{:?} {:?} events were rate limited.",
                                state.count, event_message
                            )
                        } else {
                            format!("{:?} unknown messages were rate limited.", state.count)
                        };

                        self.create_event(&id, &ctx, message);
                    }
                }

                false
            } else {
                true
            }
        } else {
            true
        }
    }

    fn on_event(&self, event: &Event, _ctx: Context<S>) {
        if is_limited(event.metadata()) {
            let mut limit_visitor = LimitVisitor::default();
            event.record(&mut limit_visitor);

            if let Some(limit) = limit_visitor.limit {
                let start = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as usize;

                let id = event.metadata().callsite();

                let mut events = self.events.write().expect("lock poisoned!");

                let state = State {
                    start,
                    count: AtomicUsize::new(1),
                    limit,
                    message: limit_visitor.message,
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

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == MESSAGE_FIELD {
            self.message = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {}
}
