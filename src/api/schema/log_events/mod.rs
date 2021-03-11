mod event;
mod notification;

use event::LogEvent;

use crate::api::{
    tap::{TapController, TapNotification, TapResult, TapSink},
    ControlSender,
};
use async_graphql::{validators::IntRange, Context, Subscription, Union};
use futures::StreamExt;
use itertools::Itertools;
use tokio::{select, stream::Stream, sync::mpsc, time};

#[derive(Union)]
/// Log event result which can be a log event or notification
pub enum LogEventResult {
    /// Log event
    LogEvent(event::LogEvent),
    /// Notification
    Notification(notification::LogEventNotification),
}

/// Convert an `api::TapResult` to the equivalent GraphQL type.
impl From<TapResult> for LogEventResult {
    fn from(t: TapResult) -> Self {
        use notification::{LogEventNotification, LogEventNotificationType};

        match t {
            TapResult::LogEvent(name, ev) => Self::LogEvent(LogEvent::new(&name, ev)),
            TapResult::Notification(name, n) => match n {
                TapNotification::Matched => Self::Notification(LogEventNotification::new(
                    &name,
                    LogEventNotificationType::Matched,
                )),
                TapNotification::NotMatched => Self::Notification(LogEventNotification::new(
                    &name,
                    LogEventNotificationType::NotMatched,
                )),
            },
        }
    }
}

#[derive(Default)]
pub struct LogEventsSubscription;

#[Subscription]
impl LogEventsSubscription {
    /// A stream of log events emitted from matched component(s)
    pub async fn output_log_events<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        component_names: Vec<String>,
        #[graphql(default = 500)] interval: i32,
        #[graphql(default = 100, validator(IntRange(min = "1", max = "10_000")))] limit: u32,
    ) -> impl Stream<Item = Vec<LogEventResult>> + 'a {
        let control_tx = ctx.data_unchecked::<ControlSender>().clone();
        create_log_events_stream(
            control_tx,
            &component_names,
            // GraphQL only supports 32 bit ints out-the-box due to JSON limitations; we're
            // casting here to separate concerns and avoid 64 bit scalar deserialization.
            interval as u64,
            limit as usize,
        )
    }
}

/// Creates a log events stream based on component names, and a provided interval. Will emit
/// control messages that bubble up the application if the sink goes away. The stream contains
/// all matching events; filtering should be done at the caller level.
fn create_log_events_stream(
    control_tx: ControlSender,
    component_names: &[String],
    interval: u64,
    limit: usize,
) -> impl Stream<Item = Vec<LogEventResult>> {
    let (tx, mut rx) = mpsc::channel(limit);
    let (mut log_tx, log_rx) = mpsc::channel::<Vec<LogEventResult>>(10);

    let tap_sink = TapSink::new(&component_names, tx);

    tokio::spawn(async move {
        // The tap controller is scoped to the stream. When it's dropped, it bubbles a control
        // message up to the signal handler to remove the ad hoc sinks from topology.
        let _control = TapController::new(control_tx, tap_sink);

        let mut interval = time::interval(time::Duration::from_millis(interval));
        let mut results: Vec<LogEventResult> = vec![];

        loop {
            select! {
                // Process `TapResults`s. A tap result could contain a `LogEvent` or a notification.
                // Notifications are emitted immediately; log events buffer until the next `interval`.
                Some(tap) = rx.next() => {
                    let tap = tap.into();

                    if let LogEventResult::Notification(_) = tap {
                        // If an error occurs when sending, the subscription has likely gone
                        // away. Break the loop to terminate the thread.
                        if let Err(_) = log_tx.send(vec![tap]).await {
                            break;
                        }
                    } else {
                        results.push(tap);
                    }
                }
                _ = interval.tick() => {
                    // If there are any existing results after the interval tick, emit.
                    if !results.is_empty() {
                        let results_len = results.len();

                        // Events are 'sampled' up to the maximum 'limit', per an even
                        // distribution of all events captured over the interval. We enumerate
                        // chunks here to ensure the very last event is always returned when
                        // limit > 1.
                        let results = results
                            .drain(..)
                            .chunks(results_len.checked_div(limit).unwrap_or(1).max(1))
                            .into_iter()
                            .enumerate()
                            .flat_map(|(i, chunk)| {
                                let mut chunk = chunk.collect_vec();
                                if matches!(i.checked_sub(1), Some(i) if i > 0 && i == results_len - 1) {
                                    chunk = chunk.into_iter().rev().collect_vec();
                                }
                                chunk.into_iter().take(1)
                            })
                            .take(limit)
                            .collect();

                        // If we get an error here, it likely means that the subscription has
                        // gone has away. This is a valid/common situation.
                        if let Err(_) = log_tx.send(results).await {
                            break;
                        }
                    }
                }
            }
        }
    });

    log_rx
}
