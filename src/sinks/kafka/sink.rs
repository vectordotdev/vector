use crate::sinks::kafka::config::KafkaSinkConfig;
use crate::sinks::util::StreamSink;
use async_graphql::futures_util::stream::BoxStream;
use crate::event::Event;

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

    }
}
