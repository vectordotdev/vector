mod log;
mod notification;

use log::LogEvent;

use crate::api::{
    tap::{TapController, TapNotification, TapResult, TapSink},
    ControlSender,
};
use async_graphql::{Context, Subscription, Union};
use futures::{channel::mpsc, StreamExt};
use tokio::{select, stream::Stream, time};

#[derive(Union)]
/// Log event result which can be a payload for log events, or an error
pub enum LogEventResult {
    /// Log event payload
    LogEvent(log::LogEvent),
    /// Log notification
    Notification(notification::LogEventNotification),
}

/// Convert an `api::TapResult` to the equivalent GraphQL type.
impl From<TapResult> for LogEventResult {
    fn from(t: TapResult) -> Self {
        use notification::{LogEventNotification, LogEventNotificationType};

        match t {
            TapResult::LogEvent(name, ev) => Self::LogEvent(LogEvent::new(&name, ev)),
            TapResult::Notification(name, n) => match n {
                TapNotification::ComponentMatched => Self::Notification(LogEventNotification::new(
                    &name,
                    LogEventNotificationType::ComponentMatched,
                )),
                TapNotification::ComponentNotMatched => Self::Notification(
                    LogEventNotification::new(&name, LogEventNotificationType::ComponentNotMatched),
                ),
            },
        }
    }
}

#[derive(Default)]
pub struct EventsSubscription;

#[Subscription]
impl EventsSubscription {
    /// A stream of log events emitted from component(s)
    pub async fn output_log_events<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        component_names: Vec<String>,
        #[graphql(default = 500)] interval: i32,
    ) -> impl Stream<Item = Vec<LogEventResult>> + 'a {
        let control_tx = ctx.data_unchecked::<ControlSender>().clone();
        create_log_events_stream(control_tx, &component_names, interval)
    }
}

/// Creates a log events stream based on component names, and a provided interval. Will emit
/// control messages that bubble up the application if the sink goes away. The stream contains
/// all matching events; filtering should be done at the caller level.
fn create_log_events_stream(
    control_tx: ControlSender,
    component_names: &[String],
    interval: i32,
) -> impl Stream<Item = Vec<LogEventResult>> {
    let (tx, mut rx) = mpsc::unbounded();
    let (mut log_tx, log_rx) = mpsc::unbounded::<Vec<LogEventResult>>();

    let tap_sink = TapSink::new(&component_names, tx);

    tokio::spawn(async move {
        // The tap controller is scoped to the stream. When it's dropped, it bubbles a control
        // message up to the signal handler to remove the ad hoc sinks from topology.
        let _control = TapController::new(control_tx, tap_sink);

        let mut interval = time::interval(time::Duration::from_millis(interval as u64));
        let mut results: Vec<LogEventResult> = vec![];

        loop {
            select! {
                // Process `TapResults`s. A tap result could contain a `LogEvent` or an error; if
                // we get an error, the subscription is dropped.
                Some(tap) = rx.next() => {
                    results.push(tap.into());
                }
                _ = interval.tick() => {
                    // If there are any existing results after the interval tick, emit.
                    if !results.is_empty() {
                        let _ = log_tx.start_send(results.drain(..).collect());
                    }
                }
            }
        }
    });

    log_rx
}
