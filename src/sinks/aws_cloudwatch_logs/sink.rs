use crate::event::{Event, LogEvent, Value};
use crate::internal_events::TemplateRenderingFailed;
use crate::sinks::aws_cloudwatch_logs::{CloudwatchKey, CloudwatchLogsError};
use crate::sinks::util::encoding::{
    Encoder, EncodingConfig, EncodingConfiguration, StandardEncodings,
};
use crate::sinks::util::processed_event::ProcessedEvent;
use crate::sinks::util::EncodedEvent;
use crate::template::Template;
use async_graphql::futures_util::stream::BoxStream;
use chrono::Utc;
use futures::future;
use futures::StreamExt;
use rusoto_logs::InputLogEvent;
use vector_core::sink::StreamSink;
use vector_core::ByteSizeOf;

// Estimated maximum size of InputLogEvent with an empty message
const EVENT_SIZE_OVERHEAD: usize = 50;
const MAX_EVENT_SIZE: usize = 256 * 1024;
const MAX_MESSAGE_SIZE: usize = MAX_EVENT_SIZE - EVENT_SIZE_OVERHEAD;

pub struct CloudwatchSink {
    group_template: Template,
    stream_template: Template,
}

impl CloudwatchSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let x = input.filter_map(|event| {
            let group = match self.group_template.render_string(&event) {
                Ok(b) => b,
                Err(error) => {
                    emit!(&TemplateRenderingFailed {
                        error,
                        field: Some("group"),
                        drop_event: true,
                    });
                    return future::ready(None);
                }
            };

            let stream = match self.stream_template.render_string(&event) {
                Ok(b) => b,
                Err(error) => {
                    emit!(&TemplateRenderingFailed {
                        error,
                        field: Some("stream"),
                        drop_event: true,
                    });
                    return future::ready(None);
                }
            };
            future::ready(Some(ProcessedEvent {
                event,
                metadata: (group, stream),
            }))
        });
        //             .map(|event| (event.size_of(), event.into_log()))
        // into_log            .filter_map(move |(event_byte_size, log)| {
        //                 future::ready(process_log(
        //                     log,
        //                     event_byte_size,
        //                     sourcetype,
        //                     source,
        //                     index,
        //                     host,
        //                     indexed_fields,
        //                 ))
        //             })
        //             .batched(self.batch_settings.into_byte_size_config())
        //             .request_builder(builder_limit, self.request_builder)
        //             .filter_map(|request| async move {
        //                 match request {
        //                     Err(e) => {
        //                         error!("Failed to build HEC Logs request: {:?}.", e);
        //                         None
        //                     }
        //                     Ok(req) => Some(req),
        //                 }
        //             })
        //             .into_driver(self.service, self.context.acker())
        //             .run().await
        unimplemented!()
    }
}

impl StreamSink for CloudwatchSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

// fn partition_encode(
//     mut event: Event,
//     encoding: &EncodingConfig<StandardEncodings>,
//     group: &Template,
//     stream: &Template,
// ) -> Option<EncodedEvent<(InputLogEvent, CloudwatchKey)>> {
//     let group = match group.render_string(&event) {
//         Ok(b) => b,
//         Err(error) => {
//             emit!(&TemplateRenderingFailed {
//                 error,
//                 field: Some("group"),
//                 drop_event: true,
//             });
//             return None;
//         }
//     };
//
//     let stream = match stream.render_string(&event) {
//         Ok(b) => b,
//         Err(error) => {
//             emit!(&TemplateRenderingFailed {
//                 error,
//                 field: Some("stream"),
//                 drop_event: true,
//             });
//             return None;
//         }
//     };
//
//     let key = CloudwatchKey { group, stream };
//
//     let byte_size = event.size_of();
//     encoding.apply_rules(&mut event);
//     let event = encode_log(event.into_log(), encoding)
//         .map_err(
//             |error| error!(message = "Could not encode event.", %error, internal_log_rate_secs = 5),
//         )
//         .ok()?;
//
//     Some(EncodedEvent::new(
//         (event, key),
//         byte_size,
//     ))
// }
//
// fn encode_log(
//     mut log: LogEvent,
//     encoding: &EncodingConfig<StandardEncodings>,
// ) -> Result<InputLogEvent, CloudwatchLogsError> {
//     let timestamp = match log.remove(log_schema().timestamp_key()) {
//         Some(Value::Timestamp(ts)) => ts.timestamp_millis(),
//         _ => Utc::now().timestamp_millis(),
//     };
//
//     encoding.codec().encode_input()
//     let message = match encoding.codec() {
//         StandardEncodings::Json => serde_json::to_string(&log).unwrap(),
//         StandardEncodings::Text => log
//             .get(log_schema().message_key())
//             .map(|v| v.to_string_lossy())
//             .unwrap_or_else(|| "".into()),
//     };
//
//     match message.len() {
//         length if length <= MAX_MESSAGE_SIZE => Ok(InputLogEvent { message, timestamp }),
//         length => Err(CloudwatchLogsError::EventTooLong { length }),
//     }
// }
