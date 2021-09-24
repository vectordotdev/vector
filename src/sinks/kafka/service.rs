use rdkafka::producer::FutureProducer;
use crate::kafka::KafkaStatisticsContext;
use tower::Service;
use std::task::{Context, Poll};
use futures::future::BoxFuture;

pub struct KafkaRequest {

}

// #[derive(Clone)]
pub struct KafkaService {
    kafka_producer: FutureProducer<KafkaStatisticsContext>
}

impl KafkaService {
    pub fn new(kafka_producer: FutureProducer<KafkaStatisticsContext>) -> KafkaService {
        KafkaService {
            kafka_producer
        }
    }
}

impl Service<()> for KafkaService {
    type Response = ();
    type Error = ();
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        todo!()
    }

    fn call(&mut self, req: ()) -> Self::Future {
        todo!()
    }
}
