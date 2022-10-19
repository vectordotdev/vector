use async_trait::async_trait;
use aws_smithy_client::SdkError;
use bytes::Bytes;

pub trait Record {
    type T;

    fn new(payload_bytes: &Bytes, partition_key: &str) -> Self;

    fn encoded_length(&self) -> usize;

    fn get(self) -> Self::T;
}

#[async_trait]
pub trait SendRecord {
    type T;
    type E;

    async fn send(&self, records: Vec<Self::T>, stream_name: String) -> Option<SdkError<Self::E>>;
}
