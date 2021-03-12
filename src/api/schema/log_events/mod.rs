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
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::cmp::Ordering;
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
        #[graphql(default = 500)] interval: u32,
        #[graphql(default = 100, validator(IntRange(min = "1", max = "10_000")))] limit: u32,
    ) -> impl Stream<Item = Vec<LogEventResult>> + 'a {
        let control_tx = ctx.data_unchecked::<ControlSender>().clone();
        create_log_events_stream(
            control_tx,
            &component_names,
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

        // A tick interval to represent when to 'cut' the results back to the client.
        let mut interval = time::interval(time::Duration::from_millis(interval));

        // Collect a vector of results, with a capacity of `limit`. As new `LogEvent`s come in,
        // they will be sampled and added to results.
        let mut results = Vec::<LogEventResult>::with_capacity(limit);

        // Random number generator to allow for sampling. Speed trumps cryptographic security here.
        // The RNG must be Send + Sync to use with the `select!` loop below.
        let mut rng = SmallRng::from_entropy();

        // Keep a count of the batch size, which will be used as a seed for random eviction
        // per the sampling strategy used below.
        let mut batch = 0;

        loop {
            select! {
                // Process `TapResults`s. A tap result could contain a `LogEvent` or a notification.
                // Notifications are emitted immediately; log events buffer until the next `interval`.
                Some(tap) = rx.next() => {
                    let tap = tap.into();

                    // Emit notifications immediately; these don't count as a 'batch'.
                    if let LogEventResult::Notification(_) = tap {
                        // If an error occurs when sending, the subscription has likely gone
                        // away. Break the loop to terminate the thread.
                        if log_tx.send(vec![tap]).await.is_err() {
                            break;
                        }
                    } else {
                        // A simple implementation of "Algorithm R" per
                        // https://en.wikipedia.org/wiki/Reservoir_sampling. As we're unable to
                        // pluck the nth result, this is chosen over the more optimal "Algorithm L"
                        // since discarding results isn't an option.
                        if limit > results.len() {
                            results.push(tap);
                        } else {
                            let random_number = rng.gen_range(0..batch);
                            if random_number < results.len() {
                                results[random_number] = tap;
                            }
                        }
                        // Increment the batch count, to be used for the next Algo R loop
                        batch += 1;
                    }
                }
                _ = interval.tick() => {
                    // If there are any existing results after the interval tick, emit.
                    if !results.is_empty() {
                        // Reset the batch count, to adjust sampling probability for the next round.
                        batch = 0;

                        // Since events will appear out of order per the random sampling
                        // strategy, drain the existing results and sort by timestamp.
                        let results = results
                            .drain(..)
                            .sorted_by(|a, b| match (a, b) {
                                (LogEventResult::LogEvent(a), LogEventResult::LogEvent(b)) => {
                                    match (a.get_timestamp(), b.get_timestamp()) {
                                        (Some(a), Some(b)) => a.cmp(b),
                                        _ => Ordering::Equal,
                                    }
                                }
                                _ => Ordering::Equal,
                            })
                            .collect();

                        // If we get an error here, it likely means that the subscription has
                        // gone has away. This is a valid/common situation.
                        if log_tx.send(results).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    log_rx
}
