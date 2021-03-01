mod log;

use log::LogEvent;

use crate::api::{
    tap::{TapController, TapError, TapResult, TapSink},
    ControlSender,
};
use async_graphql::{Context, Enum, SimpleObject, Subscription, Union};
use async_stream::stream;
use futures::{channel::mpsc, StreamExt};
use tokio::stream::Stream;

#[derive(Enum, Copy, Clone, PartialEq, Eq)]
/// Type of log event error
pub enum LogEventErrorType {
    /// Component name doesn't match a currently configured component
    ComponentInvalid,
    /// Component has been removed from topology
    ComponentGoneAway,
}

#[derive(SimpleObject)]
/// An error that occurred attempting to observe log events against a component
pub struct LogEventError {
    /// Name of the component associated with the error
    component_name: String,

    /// Type of log event error
    error_type: LogEventErrorType,
}

impl LogEventError {
    fn new(component_name: &str, error_type: LogEventErrorType) -> Self {
        Self {
            component_name: component_name.to_string(),
            error_type,
        }
    }
}

#[derive(Union)]
/// Log event result which can be a payload for log events, or an error.
pub enum LogEventResult {
    LogEvent(log::LogEvent),
    Error(LogEventError),
}

/// Convert an `api::TapResult` to the equivalent GraphQL type.
impl From<TapResult> for LogEventResult {
    fn from(t: TapResult) -> Self {
        match t {
            TapResult::LogEvent(name, ev) => Self::LogEvent(LogEvent::new(&name, ev)),
            TapResult::Error(name, err) => match err {
                TapError::ComponentInvalid => Self::Error(LogEventError::new(
                    &name,
                    LogEventErrorType::ComponentInvalid,
                )),
                TapError::ComponentGoneAway => Self::Error(LogEventError::new(
                    &name,
                    LogEventErrorType::ComponentGoneAway,
                )),
            },
            TapResult::Stop => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct EventsSubscription;

#[Subscription]
impl EventsSubscription {
    /// A stream of component(s) log events
    pub async fn log_events<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        component_names: Vec<String>,
    ) -> impl Stream<Item = LogEventResult> + 'a {
        let control_tx = ctx.data_unchecked::<ControlSender>().clone();

        let (tx, mut rx) = mpsc::unbounded();
        let tap_sink = TapSink::new(&component_names, tx);

        stream! {
            // The tap controller is scoped to the stream. When it's dropped, it bubbles a control
            // message up to the signal handler to remove the ad hoc sinks from topology.
            let _control = TapController::new(control_tx, tap_sink);

            // Process `TapResults`s. A tap result could contain a `LogEvent` or an error; if
            // we get an error, the subscription is dropped.
            while let Some(tap) = rx.next().await {
                if let TapResult::Stop = tap {
                    break;
                }
                yield tap.into();
            }
        }
    }
}
