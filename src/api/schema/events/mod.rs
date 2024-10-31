pub mod encoding;
pub mod log;
pub mod metric;
pub mod output;
pub mod trace;

use async_graphql::{Context, Subscription};
use encoding::EventEncodingType;
use futures::{stream, Stream, StreamExt};
use output::{from_tap_payload_to_output_events, OutputEventsPayload};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use tokio::{select, sync::mpsc, time};
use tokio_stream::wrappers::ReceiverStream;
use vector_lib::tap::{
    controller::{TapController, TapPatterns},
    topology::WatchRx,
};

#[derive(Debug, Default)]
pub struct EventsSubscription;

#[Subscription]
impl EventsSubscription {
    /// A stream of events emitted from matched component ID patterns
    pub async fn output_events_by_component_id_patterns<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        outputs_patterns: Vec<String>,
        inputs_patterns: Option<Vec<String>>,
        #[graphql(default = 500)] interval: u32,
        #[graphql(default = 100, validator(minimum = 1, maximum = 10_000))] limit: u32,
    ) -> impl Stream<Item = Vec<OutputEventsPayload>> + 'a {
        let watch_rx = ctx.data_unchecked::<WatchRx>().clone();

        let patterns = TapPatterns {
            for_outputs: outputs_patterns.into_iter().collect(),
            for_inputs: inputs_patterns.unwrap_or_default().into_iter().collect(),
        };
        // Client input is confined to `u32` to provide sensible bounds.
        create_events_stream(watch_rx, patterns, interval as u64, limit as usize)
    }
}

/// Creates an events stream based on component ids, and a provided interval. Will emit
/// control messages that bubble up the application if the sink goes away. The stream contains
/// all matching events; filtering should be done at the caller level.
pub(crate) fn create_events_stream(
    watch_rx: WatchRx,
    patterns: TapPatterns,
    interval: u64,
    limit: usize,
) -> impl Stream<Item = Vec<OutputEventsPayload>> {
    // Channel for receiving individual tap payloads. Since we can process at most `limit` per
    // interval, this is capped to the same value.
    let (tap_tx, tap_rx) = mpsc::channel(limit);
    let mut tap_rx = ReceiverStream::new(tap_rx)
        .flat_map(|payload| stream::iter(from_tap_payload_to_output_events(payload)));

    // The resulting vector of `Event` sent to the client. Only one result set will be streamed
    // back to the client at a time. This value is set higher than `1` to prevent blocking the event
    // pipeline on slower client connections, but low enough to apply a modest cap on mem usage.
    let (event_tx, event_rx) = mpsc::channel::<Vec<OutputEventsPayload>>(10);

    tokio::spawn(async move {
        // Create a tap controller. When this drops out of scope, clean up will be performed on the
        // event handlers and topology observation that the tap controller provides.
        let _tap_controller = TapController::new(watch_rx, tap_tx, patterns);

        // A tick interval to represent when to 'cut' the results back to the client.
        let mut interval = time::interval(time::Duration::from_millis(interval));

        // Temporary structure to hold sortable values of `Event`.
        struct SortableOutputEventsPayload {
            batch: usize,
            payload: OutputEventsPayload,
        }

        // Collect a vector of results, with a capacity of `limit`. As new `Event`s come in,
        // they will be sampled and added to results.
        let mut results = Vec::<SortableOutputEventsPayload>::with_capacity(limit);

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
                Some(payload) = tap_rx.next() => {
                    // Emit notifications immediately; these don't count as a 'batch'.
                    if let OutputEventsPayload::Notification(_) = payload {
                        // If an error occurs when sending, the subscription has likely gone
                        // away. Break the loop to terminate the thread.
                        if let Err(err) = event_tx.send(vec![payload]).await {
                            debug!(message = "Couldn't send notification.", error = ?err);
                            break;
                        }
                    } else {
                        // Wrap tap in a 'sortable' wrapper, using the batch as a key, to
                        // re-sort after random eviction.
                        let payload = SortableOutputEventsPayload { batch, payload };

                        // A simple implementation of "Algorithm R" per
                        // https://en.wikipedia.org/wiki/Reservoir_sampling. As we're unable to
                        // pluck the nth result, this is chosen over the more optimal "Algorithm L"
                        // since discarding results isn't an option.
                        if limit > results.len() {
                            results.push(payload);
                        } else {
                            let random_number = rng.gen_range(0..batch);
                            if random_number < results.len() {
                                results[random_number] = payload;
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
                        results.sort_by_key(|r| r.batch);
                        let results = results.drain(..)
                            .map(|r| r.payload)
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

    ReceiverStream::new(event_rx)
}
