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
pub enum EventErrorType {
    ComponentInvalid,
    ComponentGoneAway,
}

#[derive(SimpleObject)]
pub struct EventError {
    error_type: EventErrorType,
}

impl EventError {
    fn new(error_type: EventErrorType) -> Self {
        Self { error_type }
    }
}

#[derive(Union)]
pub enum EventResult {
    LogEvent(log::LogEvent),
    Error(EventError),
}

impl From<TapResult> for EventResult {
    fn from(t: TapResult) -> Self {
        match t {
            TapResult::LogEvent(ev) => Self::LogEvent(LogEvent::new(ev)),
            TapResult::Error(err) => match err {
                TapError::ComponentInvalid => {
                    Self::Error(EventError::new(EventErrorType::ComponentInvalid))
                }
                TapError::ComponentGoneAway => {
                    Self::Error(EventError::new(EventErrorType::ComponentGoneAway))
                }
            },
        }
    }
}

#[derive(Default)]
pub struct EventsSubscription;

#[Subscription]
impl EventsSubscription {
    /// A stream of a component's log events
    pub async fn log_events<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        component_name: String,
    ) -> impl Stream<Item = EventResult> + 'a {
        let control_tx = ctx.data_unchecked::<ControlSender>().clone();

        let (tx, mut rx) = mpsc::unbounded();
        let tap_sink = TapSink::new(&component_name, tx);

        stream! {
            let _control = TapController::new(control_tx, tap_sink);
            while let Some(tap) = rx.next().await {
                let is_error = tap.is_error();
                yield tap.into();

                if is_error {
                    break;
                }
            }
        }
    }
}
