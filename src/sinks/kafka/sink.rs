use crate::sinks::util::StreamSink;
use crate::event::Event;
use futures::stream::BoxStream;
use async_trait::async_trait;
use super::config::KafkaSinkConfig;
use vector_core::buffers::Acker;

pub struct KafkaSink {
    headers_key: Option<String>,
    acker: Acker,
}

impl KafkaSink {
    pub fn new(config: KafkaSinkConfig, acker: Acker) -> crate::Result<Self> {
        Ok(KafkaSink {
            headers_key: config.headers_key,
            acker
        })
    }
}

#[async_trait]
impl StreamSink for KafkaSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        println!("Running kafka sink");
        Ok(())
        // todo!();
    }
}
