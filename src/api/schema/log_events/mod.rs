mod event;
mod notification;

use event::LogEvent;

use crate::{
    api::tap::{TapNotification, TapPayload, TapSink},
    topology::WatchRx,
};

use async_graphql::{validators::IntRange, Context, Subscription, Union};
use futures::StreamExt;
use itertools::Itertools;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::cmp::Ordering;
use tokio::{select, stream::Stream, sync::mpsc, time};

#[derive(Union, Debug)]
/// Log event payload which can be a log event or notification
pub enum LogEventPayload {
    /// Log event
    LogEvent(event::LogEvent),
    /// Notification
    Notification(notification::LogEventNotification),
}

/// Convert an `api::TapPayload` to the equivalent GraphQL type.
impl From<TapPayload> for LogEventPayload {
    fn from(t: TapPayload) -> Self {
        use notification::{LogEventNotification, LogEventNotificationType};

        match t {
            TapPayload::LogEvent(name, ev) => Self::LogEvent(LogEvent::new(&name, ev)),
            TapPayload::Notification(name, n) => match n {
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

#[derive(Debug, Default)]
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
    ) -> impl Stream<Item = Vec<LogEventPayload>> + 'a {
        let watch_rx = ctx.data_unchecked::<WatchRx>().clone();

        // Client input is confined to `u32` to provide sensible bounds.
        create_log_events_stream(watch_rx, component_names, interval as u64, limit as usize)
    }
}

/// Creates a log events stream based on component names, and a provided interval. Will emit
/// control messages that bubble up the application if the sink goes away. The stream contains
/// all matching events; filtering should be done at the caller level.
fn create_log_events_stream(
    watch_rx: WatchRx,
    component_names: Vec<String>,
    interval: u64,
    limit: usize,
) -> impl Stream<Item = Vec<LogEventPayload>> {
    // Channel for receiving individual tap payloads. Since we can process at most `limit` per
    // interval, this is capped to the same value.
    let (tap_tx, mut tap_rx) = mpsc::channel(limit);

    // The resulting vector of `LogEventPayload` sent to the client. Only one result set will be streamed
    // back to the client at a time. This value is set higher than `1` to prevent blocking the event
    // pipeline on slower client connections, but low enough to apply a modest cap on mem usage.
    let (mut log_tx, log_rx) = mpsc::channel::<Vec<LogEventPayload>>(10);

    tokio::spawn(async move {
        // Create a tap sink. When this drops out of scope, clean up will be performed on the
        // event handlers and topology observation that the tap sink provides.
        let _tap_sink = TapSink::new(watch_rx, tap_tx, &component_names);

        // A tick interval to represent when to 'cut' the results back to the client.
        let mut interval = time::interval(time::Duration::from_millis(interval));

        // Collect a vector of results, with a capacity of `limit`. As new `LogEvent`s come in,
        // they will be sampled and added to results.
        let mut results = Vec::<LogEventPayload>::with_capacity(limit);

        // Random number generator to allow for sampling. Speed trumps cryptographic security here.
        // The RNG must be Send + Sync to use with the `select!` loop below, hence `SmallRng`.
        let mut rng = SmallRng::from_entropy();

        // Keep a count of the batch size, which will be used as a seed for random eviction
        // per the sampling strategy used below.
        let mut batch = 0;

        loop {
            select! {
                // Process `TapPayload`s. A tap payload could contain a `LogEvent` or a notification.
                // Notifications are emitted immediately; log events buffer until the next `interval`.
                Some(tap) = tap_rx.next() => {
                    let tap = tap.into();

                    // Emit notifications immediately; these don't count as a 'batch'.
                    if let LogEventPayload::Notification(_) = tap {
                        // If an error occurs when sending, the subscription has likely gone
                        // away. Break the loop to terminate the thread.
                        if let Err(err) = log_tx.send(vec![tap]).await {
                            debug!(message = "Couldn't send notification.", error = ?err);
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
                        // Increment the batch count, to be used for the next Algo R loop.
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
                                (LogEventPayload::LogEvent(a), LogEventPayload::LogEvent(b)) => {
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
                        if let Err(err) = log_tx.send(results).await {
                            debug!(message = "Couldn't send log events.", error = ?err);
                            break;
                        }
                    }
                }
            }
        }
    });

    log_rx
}
