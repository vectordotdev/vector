use ansi_term::Colour;
use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        RwLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_trace_core::{
    callsite::Identifier,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
    Event, Interest, Level, Metadata, Subscriber,
};

pub struct LimitSubscriber<S> {
    inner: S,
    events: RwLock<HashMap<Identifier, (AtomicUsize, AtomicUsize)>>,
}

#[derive(Default)]
struct LimitVisitor {
    limit: Option<usize>,
}

impl LimitVisitor {
    pub fn into_limit(self) -> Option<usize> {
        self.limit
    }
}

impl<S> LimitSubscriber<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            events: RwLock::new(HashMap::new()),
        }
    }
}

impl<S: Subscriber> Subscriber for LimitSubscriber<S> {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn new_span(&self, span: &Attributes) -> Id {
        self.inner.new_span(span)
    }

    fn record(&self, span: &Id, values: &Record) {
        self.inner.record(span, values);
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        self.inner.record_follows_from(span, follows);
    }

    fn event(&self, event: &Event) {
        if event.fields().any(|f| f.name() == "rate_limit") {
            let mut limit_visitor = LimitVisitor::default();
            event.record(&mut limit_visitor);

            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as usize;

            if let Some(limit) = limit_visitor.into_limit() {
                let id = event.metadata().callsite();

                let events = self.events.read().unwrap();
                if let Some((count, start)) = events.get(&id) {
                    let local_count = count.fetch_add(1, Ordering::Relaxed);

                    if ts - start.load(Ordering::Relaxed) < limit {
                        // If we have seen this event more than once,
                        // then lets early return to avoid passing this
                        // event to the inner subscriber.
                        if local_count >= 1 {
                            return;
                        }
                    } else {
                        drop(events);

                        let mut events = self.events.write().unwrap();
                        events.remove(&id);
                        drop(events);

                        let meta = event.metadata();

                        let mut visitor = FmtVisitor::default();
                        event.record(&mut visitor);

                        let message = if let Some(message) = &visitor.message {
                            &message
                        } else {
                            "unknown event"
                        };

                        // We need to replicate the way that fmt logs events
                        // because we currently can not create new fresh events.
                        println!(
                            "{} {} {:?} {:?} logs were rate limited.",
                            FmtLevel(meta.level()),
                            meta.target(),
                            local_count,
                            message
                        );

                        return;
                    }
                } else {
                    drop(events);
                    let count = AtomicUsize::new(1);
                    let ts = AtomicUsize::new(ts as usize);
                    let mut map = self.events.write().unwrap();
                    map.insert(id, (count, ts));
                }
            }
        }

        self.inner.event(event);
    }

    fn enter(&self, span: &Id) {
        self.inner.enter(span);
    }

    fn exit(&self, span: &Id) {
        self.inner.exit(span);
    }

    fn register_callsite(&self, metadata: &Metadata) -> Interest {
        self.inner.register_callsite(metadata)
    }

    fn clone_span(&self, id: &Id) -> Id {
        self.inner.clone_span(id)
    }

    fn drop_span(&self, id: Id) {
        self.inner.drop_span(id.clone());
    }
}

impl Visit for LimitVisitor {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "rate_limit" {
            self.limit = Some(value as usize);
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &fmt::Debug) {}
}

struct FmtLevel<'a>(&'a Level);

impl<'a> fmt::Display for FmtLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self.0 {
            Level::TRACE => write!(f, "{}", Colour::Purple.paint("TRACE")),
            Level::DEBUG => write!(f, "{}", Colour::Blue.paint("DEBUG")),
            Level::INFO => write!(f, "{}", Colour::Green.paint(" INFO")),
            Level::WARN => write!(f, "{}", Colour::Yellow.paint(" WARN")),
            Level::ERROR => write!(f, "{}", Colour::Red.paint("ERROR")),
        }
    }
}

#[derive(Default)]
pub struct FmtVisitor {
    pub message: Option<String>,
}

impl Visit for FmtVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &fmt::Debug) {}
}
