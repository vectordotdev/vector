mod log;

use log::LogEvent;

use crate::api::{tap::TapSink, ControlMessage, ControlSender};
use async_graphql::{Context, Subscription};
use async_stream::stream;
use tokio::{stream::Stream, sync::mpsc};

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
        let mut control_tx = ctx.data_unchecked::<ControlSender>().clone();

        let (tx, mut rx) = mpsc::channel(100);
        let tap_sink = TapSink::new(&component_name, tx);

        let _ = control_tx.send(ControlMessage::Tap(tap_sink.start())).await;

        stream! {
            while let Some(ev) = rx.recv().await {
                yield LogEvent::new(ev)
            }
        }
    }
}
