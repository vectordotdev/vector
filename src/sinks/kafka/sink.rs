use crate::sinks::util::StreamSink;
use crate::event::Event;
use futures::stream::BoxStream;
use async_trait::async_trait;
use super::config::KafkaSinkConfig;

pub struct KafkaSink {
    headers_key: Option<String>,
}

impl KafkaSink {
    pub fn new(config: KafkaSinkConfig) -> crate::Result<Self> {
        Ok(KafkaSink {
            headers_key: config.headers_key
        })
    }
}

#[async_trait]
impl StreamSink for KafkaSink {
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {

        todo!();
    }
}
