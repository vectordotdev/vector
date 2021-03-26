mod event;
mod log_event;

use event::Event;

use crate::{api::tap::TapSink, topology::WatchRx};

use async_graphql::{validators::IntRange, Context, Subscription};
use futures::StreamExt;
use itertools::Itertools;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use tokio::{select, stream::Stream, sync::mpsc, time};

#[derive(Debug, Default)]
pub struct EventsSubscription;

#[Subscription]
impl EventsSubscription {
    /// A stream of events emitted from matched component(s)
    pub async fn output_events<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        component_names: Vec<String>,
        #[graphql(default = 500)] interval: u32,
        #[graphql(default = 100, validator(IntRange(min = "1", max = "10_000")))] limit: u32,
    ) -> impl Stream<Item = Vec<Event>> + 'a {
        let watch_rx = ctx.data_unchecked::<WatchRx>().clone();

        // Client input is confined to `u32` to provide sensible bounds.
        create_events_stream(watch_rx, component_names, interval as u64, limit as usize)
    }
}

/// Creates an events stream based on component names, and a provided interval. Will emit
/// control messages that bubble up the application if the sink goes away. The stream contains
/// all matching events; filtering should be done at the caller level.
fn create_events_stream(
    watch_rx: WatchRx,
    component_names: Vec<String>,
    interval: u64,
    limit: usize,
) -> impl Stream<Item = Vec<Event>> {
    // Channel for receiving individual tap payloads. Since we can process at most `limit` per
    // interval, this is capped to the same value.
    let (tap_tx, mut tap_rx) = mpsc::channel(limit);

    // The resulting vector of `Event` sent to the client. Only one result set will be streamed
    // back to the client at a time. This value is set higher than `1` to prevent blocking the event
    // pipeline on slower client connections, but low enough to apply a modest cap on mem usage.
    let (mut event_tx, event_rx) = mpsc::channel::<Vec<Event>>(10);

    tokio::spawn(async move {
        // Create a tap sink. When this drops out of scope, clean up will be performed on the
        // event handlers and topology observation that the tap sink provides.
        let _tap_sink = TapSink::new(watch_rx, tap_tx, &component_names);

        // A tick interval to represent when to 'cut' the results back to the client.
        let mut interval = time::interval(time::Duration::from_millis(interval));

        // Temporary structure to hold sortable values of `Event`.
        struct SortableEvent {
            batch: usize,
            event: Event,
        }

        // Collect a vector of results, with a capacity of `limit`. As new `Event`s come in,
        // they will be sampled and added to results.
        let mut results = Vec::<SortableEvent>::with_capacity(limit);

        // Random number generator to allow for sampling. Speed trumps cryptographic security here.
        // The RNG must be Send + Sync to use with the `select!` loop below, hence `SmallRng`.
        let mut rng = SmallRng::from_entropy();

        // Keep a count of the batch size, which will be used as a seed for random eviction
        // per the sampling strategy used below.
        let mut batch = 0;

        loop {
            select! {
                // Process `TapPayload`s. A tap payload could contain log/metric events or a
                // notification. Notifications are emitted immediately; events buffer until
                // the next `interval`.
                Some(event) = tap_rx.next() => {
                    let event = event.into();

                    // Emit notifications immediately; these don't count as a 'batch'.
                    if let Event::Notification(_) = event {
                        // If an error occurs when sending, the subscription has likely gone
                        // away. Break the loop to terminate the thread.
                        if let Err(err) = event_tx.send(vec![event]).await {
                            debug!(message = "Couldn't send notification.", error = ?err);
                            break;
                        }
                    } else {
                        // Wrap tap in a 'sortable' wrapper, using the batch as a key, to
                        // re-sort after random eviction.
                        let event = SortableEvent { batch, event };

                        // A simple implementation of "Algorithm R" per
                        // https://en.wikipedia.org/wiki/Reservoir_sampling. As we're unable to
                        // pluck the nth result, this is chosen over the more optimal "Algorithm L"
                        // since discarding results isn't an option.
                        if limit > results.len() {
                            results.push(event);
                        } else {
                            let random_number = rng.gen_range(0..batch);
                            if random_number < results.len() {
                                results[random_number] = event;
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
                            .sorted_by_key(|r| r.batch)
                            .map(|r| r.event)
                            .collect();

                        // If we get an error here, it likely means that the subscription has
                        // gone has away. This is a valid/common situation.
                        if let Err(err) = event_tx.send(results).await {
                            debug!(message = "Couldn't send events.", error = ?err);
                            break;
                        }
                    }
                }
            }
        }
    });

    event_rx
}
