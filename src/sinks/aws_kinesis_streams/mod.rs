mod integration_tests;
mod tests;
mod config;
mod service;
mod sink;
mod request_builder;

use crate::{
    config::{
        log_schema, DataType, GenerateConfig, ProxyConfig, SinkConfig, SinkContext, SinkDescription,
    },
    event::Event,
    internal_events::AwsKinesisStreamsEventSent,
    rusoto::{self, AwsAuthentication, RegionOrEndpoint},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        sink::Response,
        BatchConfig, BatchSettings, Compression, EncodedEvent, EncodedLength, TowerRequestConfig,
        VecBuffer,
    },
};
use bytes::Bytes;
use futures::{future::BoxFuture, stream, FutureExt, Sink, SinkExt, StreamExt, TryFutureExt};
use rand::random;
use rusoto_core::RusotoError;
use rusoto_kinesis::{
    DescribeStreamInput, Kinesis, KinesisClient, PutRecordsError, PutRecordsInput,
    PutRecordsOutput, PutRecordsRequestEntry,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    convert::TryInto,
    fmt,
    task::{Context, Poll},
};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::ByteSizeOf;
use config::KinesisSinkConfig;


inventory::submit! {
    SinkDescription::new::<KinesisSinkConfig>("sinks.aws_kinesis_streams")
}


// impl Response for PutRecordsOutput {}





// fn encode_event(
//     mut event: Event,
//     partition_key_field: &Option<String>,
//     encoding: &EncodingConfig<Encoding>,
// ) -> Option<EncodedEvent<PutRecordsRequestEntry>> {
//     // let partition_key = if let Some(partition_key_field) = partition_key_field {
//     //     if let Some(v) = event.as_log().get(&partition_key_field) {
//     //         v.to_string_lossy()
//     //     } else {
//     //         warn!(
//     //             message = "Partition key does not exist; dropping event.",
//     //             %partition_key_field,
//     //             internal_log_rate_secs = 30,
//     //         );
//     //         return None;
//     //     }
//     // } else {
//     //     gen_partition_key()
//     // };
//
//     // let partition_key = if partition_key.len() >= 256 {
//     //     partition_key[..256].to_string()
//     // } else {
//     //     partition_key
//     // };
//
//     let byte_size = event.size_of();
//     encoding.apply_rules(&mut event);
//
//     let log = event.into_log();
//     let data = match encoding.codec() {
//         Encoding::Json => serde_json::to_vec(&log).expect("Error encoding event as json."),
//         Encoding::Text => log
//             .get(log_schema().message_key())
//             .map(|v| v.as_bytes().to_vec())
//             .unwrap_or_default(),
//     };
//
//     Some(EncodedEvent::new(
//         PutRecordsRequestEntry {
//             data: Bytes::from(data),
//             partition_key,
//             ..Default::default()
//         },
//         byte_size,
//     ))
// }

// fn gen_partition_key() -> String {
//     random::<[char; 16]>()
//         .iter()
//         .fold(String::new(), |mut s, c| {
//             s.push(*c);
//             s
//         })
// }



