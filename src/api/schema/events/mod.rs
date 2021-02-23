mod log;

use log::LogEvent;

use crate::api::{
    tap::{TapController, TapSink},
    ControlSender,
};
use async_graphql::{Context, Subscription};
use async_stream::stream;
use futures::{channel::mpsc, StreamExt};
use tokio::stream::Stream;

#[derive(Default)]
pub struct EventsSubscription;

#[Subscription]
impl EventsSubscription {
    /// A stream of a component's log events
    pub async fn log_events<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        component_name: String,
    ) -> impl Stream<Item = LogEvent> + 'a {
        let control_tx = ctx.data_unchecked::<ControlSender>().clone();

        let (tx, mut rx) = mpsc::unbounded();
        let tap_sink = TapSink::new(&component_name, tx);

        stream! {
            let _control = TapController::new(control_tx, tap_sink);
            while let Some(ev) = rx.next().await {
                yield LogEvent::new(ev);
            }
        }
    }
}
