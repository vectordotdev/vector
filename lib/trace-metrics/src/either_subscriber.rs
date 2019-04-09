use tokio_trace_core::{span, Event, Interest, Metadata, Subscriber};

pub enum EitherSubscriber<L, R> {
    Left(L),
    Right(R),
}

impl<R: Subscriber, L: Subscriber> Subscriber for EitherSubscriber<L, R> {
    fn register_callsite(&self, metadata: &Metadata) -> Interest {
        match self {
            EitherSubscriber::Left(l) => l.register_callsite(metadata),
            EitherSubscriber::Right(r) => r.register_callsite(metadata),
        }
    }

    fn enabled(&self, metadata: &Metadata) -> bool {
        match self {
            EitherSubscriber::Left(l) => l.enabled(metadata),
            EitherSubscriber::Right(r) => r.enabled(metadata),
        }
    }

    fn new_span(&self, span: &span::Attributes) -> span::Id {
        match self {
            EitherSubscriber::Left(l) => l.new_span(span),
            EitherSubscriber::Right(r) => r.new_span(span),
        }
    }

    fn record(&self, span: &span::Id, values: &span::Record) {
        match self {
            EitherSubscriber::Left(l) => l.record(span, values),
            EitherSubscriber::Right(r) => r.record(span, values),
        }
    }

    fn record_follows_from(&self, span: &span::Id, follows: &span::Id) {
        match self {
            EitherSubscriber::Left(l) => l.record_follows_from(span, follows),
            EitherSubscriber::Right(r) => r.record_follows_from(span, follows),
        }
    }

    fn event(&self, event: &Event) {
        match self {
            EitherSubscriber::Left(l) => l.event(event),
            EitherSubscriber::Right(r) => r.event(event),
        }
    }

    fn enter(&self, span: &span::Id) {
        match self {
            EitherSubscriber::Left(l) => l.enter(span),
            EitherSubscriber::Right(r) => r.enter(span),
        }
    }

    fn exit(&self, span: &span::Id) {
        match self {
            EitherSubscriber::Left(l) => l.exit(span),
            EitherSubscriber::Right(r) => r.exit(span),
        }
    }
    fn clone_span(&self, id: &span::Id) -> span::Id {
        match self {
            EitherSubscriber::Left(l) => l.clone_span(id),
            EitherSubscriber::Right(r) => r.clone_span(id),
        }
    }
    fn drop_span(&self, id: span::Id) {
        match self {
            EitherSubscriber::Left(l) => l.drop_span(id),
            EitherSubscriber::Right(r) => r.drop_span(id),
        }
    }
}
