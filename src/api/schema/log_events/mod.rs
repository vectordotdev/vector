mod event;
mod notification;

use event::LogEvent;

use crate::api::{
    tap::{TapController, TapNotification, TapResult, TapSink},
    ControlSender,
};
use async_graphql::{validators::IntRange, Context, Enum, InputObject, Subscription, Union};
use futures::{channel::mpsc, StreamExt};
use itertools::Itertools;
use tokio::{select, stream::Stream, time};

#[derive(Enum, Copy, Clone, PartialEq, Eq)]
/// Where to sample log events from
pub enum LogEventSampleWhere {
    /// Start of the batch of log events
    Head,
    /// End of the batch of log events
    Tail,
    /// Even distribution of the batch of log events
    Range,
}

#[derive(InputObject)]
/// Control how log event batches are sampled
pub struct LogEventSample {
    #[graphql(name = "where")]
    /// Where to sample log events from
    sample_where: LogEventSampleWhere,

    /// Maximum number of requested log events
    #[graphql(validator(IntRange(min = "1", max = "1_000")))]
    max: usize,
}

impl Default for LogEventSample {
    fn default() -> Self {
        LogEventSample {
            sample_where: LogEventSampleWhere::Head,
            max: 100,
        }
    }
}

#[derive(Union)]
/// Log event result which can be a payload for log events, or an error
pub enum LogEventResult {
    /// Log event payload
    LogEvent(event::LogEvent),
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
pub struct LogEventsSubscription;

#[Subscription]
impl LogEventsSubscription {
    /// A stream of log events emitted from matched component(s)
    pub async fn output_log_events<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        component_names: Vec<String>,
        #[graphql(default = 500)] interval: i32,
        #[graphql(default)] sample: LogEventSample,
    ) -> impl Stream<Item = Vec<LogEventResult>> + 'a {
        let control_tx = ctx.data_unchecked::<ControlSender>().clone();
        create_log_events_stream(control_tx, &component_names, interval as u64, sample)
    }
}

/// Creates a log events stream based on component names, and a provided interval. Will emit
/// control messages that bubble up the application if the sink goes away. The stream contains
/// all matching events; filtering should be done at the caller level.
fn create_log_events_stream(
    control_tx: ControlSender,
    component_names: &[String],
    interval: u64,
    sample: LogEventSample,
) -> impl Stream<Item = Vec<LogEventResult>> {
    let (tx, mut rx) = mpsc::unbounded();
    let (mut log_tx, log_rx) = mpsc::unbounded::<Vec<LogEventResult>>();

    let tap_sink = TapSink::new(&component_names, tx);

    tokio::spawn(async move {
        // The tap controller is scoped to the stream. When it's dropped, it bubbles a control
        // message up to the signal handler to remove the ad hoc sinks from topology.
        let _control = TapController::new(control_tx, tap_sink);

        let mut interval = time::interval(time::Duration::from_millis(interval));
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
                        let results = results.drain(..);
                        let results_len = results.len();

                        // Take from the head, tail or a range of the results.
                        let results = match sample.sample_where {
                            LogEventSampleWhere::Head => results.take(sample.max).collect(),
                            LogEventSampleWhere::Tail => results.rev().take(sample.max).collect(),
                            LogEventSampleWhere::Range => {
                                // For a 'range', we split the captured events into chunks,
                                // returning the first event from each chunk-- except the *last*
                                // chunk, which returns the final record. This means the user
                                // always gets at least the first and last result if they've
                                // requested a max > 1.
                                results
                                    .chunks(
                                        results_len
                                            .checked_div(sample.max)
                                            .unwrap_or(1)
                                            .max(1),
                                    )
                                    .into_iter()
                                    .enumerate()
                                    .flat_map(|(i, chunk)| {
                                        let mut chunk = chunk.collect_vec();
                                        if matches!(i.checked_sub(1), Some(i) if i > 0 && i == results_len - 1) {
                                            chunk = chunk.into_iter().rev().collect_vec();
                                        }
                                        chunk.into_iter().take(1)
                                    })
                                    .take(sample.max)
                                    .collect()
                            }
                        };

                        let _ = log_tx.start_send(results);
                    }
                }
            }
        }
    });

    log_rx
}
