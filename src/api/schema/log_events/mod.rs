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
    /// Start of this interval's log events
    Head,
    /// End of this interval's log events
    Tail,
    /// Even distribution of this interval's log events
    Range,
}

#[derive(InputObject)]
pub struct LogEventSample {
    #[graphql(name = "where")]
    sample_where: LogEventSampleWhere,

    #[graphql(validator(IntRange(min = "1", max = "1_000")))]
    value: usize,
}

impl Default for LogEventSample {
    fn default() -> Self {
        LogEventSample {
            sample_where: LogEventSampleWhere::Head,
            value: 100,
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
    /// A stream of log events emitted from component(s)
    pub async fn output_log_events<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        component_names: Vec<String>,
        #[graphql(default = 500)] interval: i32,
        #[graphql(default)] sample: LogEventSample,
    ) -> impl Stream<Item = Vec<LogEventResult>> + 'a {
        let control_tx = ctx.data_unchecked::<ControlSender>().clone();
        create_log_events_stream(control_tx, &component_names, interval, sample)
    }
}

/// Creates a log events stream based on component names, and a provided interval. Will emit
/// control messages that bubble up the application if the sink goes away. The stream contains
/// all matching events; filtering should be done at the caller level.
fn create_log_events_stream(
    control_tx: ControlSender,
    component_names: &[String],
    interval: i32,
    sample: LogEventSample,
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
                        let results = results.drain(..).into_iter();
                        let results_len = results.len();

                        let results = match sample.sample_where {
                            LogEventSampleWhere::Head => results.take(sample.value).collect(),
                            LogEventSampleWhere::Tail => results.rev().take(sample.value).collect(),
                            LogEventSampleWhere::Range => {
                                results
                                    .chunks(
                                        results_len
                                            .checked_div(sample.value)
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
                                    .take(sample.value)
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
