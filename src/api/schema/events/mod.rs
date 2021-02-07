mod log;

use crate::sinks::tap::TapController;
use async_graphql::{Context, Subscription};
use async_stream::stream;
use log::LogEvent;
use std::sync::Arc;
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
        stream! {
            if let Some(mut receiver) = ctx
                .data_unchecked::<Arc<TapController>>()
                .subscribe(&component_name)
                {
                    while let Ok(ev) = receiver.recv().await {
                        yield LogEvent::new(ev)
                    }
                }
        }
    }
}
