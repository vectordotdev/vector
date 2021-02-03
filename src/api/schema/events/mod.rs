mod log;

use crate::event::EventInspector;
use async_graphql::{Context, Subscription};
use async_stream::stream;
use serde_json::json;
use std::sync::Arc;
use tokio::stream::{self, Stream};

#[derive(Default)]
pub struct EventsSubscription;

#[Subscription]
impl EventsSubscription {
    /// A stream of component log events
    pub async fn events<'a>(
        &'a self,
        ctx: &'a Context<'a>,
        component_name: String,
    ) -> impl Stream<Item = String> + 'a {
        stream! {
            if let Some(mut receiver) = ctx
                .data_unchecked::<Arc<EventInspector>>()
                .subscribe(&component_name)
                {
                    while let Ok(msg) = receiver.recv().await {
                        yield json!(msg).to_string();
                    }
                }
        }
    }
}
