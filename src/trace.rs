use crate::metrics;
use tracing::{
    dispatcher::{set_global_default, Dispatch},
    span::Span,
};
use tracing_limit::Limit;
use tracing_log::LogTracer;
use tracing_metrics::MetricsSubscriber;
use tracing_subscriber::{layer::SubscriberExt, FmtSubscriber};

pub use tracing_futures::Instrument;
pub use tracing_tower::{InstrumentableService, InstrumentedService};

pub fn init(color: bool, json: bool, levels: &str, metrics: Option<metrics::Sink>) {
    let dispatch = if json {
        let subscriber = FmtSubscriber::builder()
            .with_env_filter(levels)
            .json()
            .finish()
            .with(Limit::default());

        if let Some(sink) = metrics {
            Dispatch::new(MetricsSubscriber::new(subscriber, sink))
        } else {
            Dispatch::new(subscriber)
        }
    } else {
        let subscriber = FmtSubscriber::builder()
            .with_ansi(color)
            .with_env_filter(levels)
            .finish()
            .with(Limit::default());

        if let Some(sink) = metrics {
            Dispatch::new(MetricsSubscriber::new(subscriber, sink))
        } else {
            Dispatch::new(subscriber)
        }
    };

    let _ = LogTracer::init();
    let _ = set_global_default(dispatch);
}

pub fn current_span() -> Span {
    Span::current()
}
