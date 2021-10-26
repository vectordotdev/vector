use async_graphql::futures_util::stream::BoxStream;
use futures::StreamExt;
use crate::event::Event;
use crate::sinks::util::{SinkBuilderExt, StreamSink};

pub struct KinesisSink {

}


impl StreamSink for KinesisSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let driver = input
            .map(|event|{
                // Panic: This sink only accepts Logs, so this should never panic
                event.into_log()
            })
            .batched();
    }
}
