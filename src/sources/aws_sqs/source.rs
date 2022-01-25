use std::{collections::HashMap, io::Cursor, panic, str::FromStr};

use async_stream::stream;
use aws_sdk_sqs::{
    model::{DeleteMessageBatchRequestEntry, MessageSystemAttributeName, QueueAttributeName},
    Client as SqsClient,
};
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use futures::{FutureExt, Stream, StreamExt};
use tokio::{pin, select, time::Duration};
use tokio_util::codec::FramedRead;
use vector_core::internal_event::EventsReceived;

use super::events::*;
use crate::{
    codecs::Decoder,
    config::log_schema,
    event::{BatchNotifier, BatchStatus, Event},
    shutdown::ShutdownSignal,
    sources::util::StreamDecodingError,
    vector_core::ByteSizeOf,
    SourceSender,
};

// This is the maximum SQS supports in a single batch request
const MAX_BATCH_SIZE: i32 = 10;

#[derive(Clone)]
pub struct SqsSource {
    pub client: SqsClient,
    pub queue_url: String,
    pub decoder: Decoder,
    pub poll_secs: u32,
    pub concurrency: u32,
    pub acknowledgements: bool,
}

impl SqsSource {
    pub async fn run(self, out: SourceSender, shutdown: ShutdownSignal) -> Result<(), ()> {
        let mut task_handles = vec![];

        for _ in 0..self.concurrency {
            let source = self.clone();
            let shutdown = shutdown.clone().fuse();
            let mut out = out.clone();
            task_handles.push(tokio::spawn(async move {
                pin!(shutdown);
                loop {
                    select! {
                        _ = &mut shutdown => break,
                        _ = source.run_once(&mut out, self.acknowledgements) => {},
                    }
                }
            }));
        }

        // Wait for all of the processes to finish.  If any one of them panics, we resume
        // that panic here to properly shutdown Vector.
        for task_handle in task_handles.drain(..) {
            if let Err(e) = task_handle.await {
                if e.is_panic() {
                    panic::resume_unwind(e.into_panic());
                }
            }
        }
        Ok(())
    }

    async fn run_once(&self, out: &mut SourceSender, acknowledgements: bool) {
        let result = self
            .client
            .receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(MAX_BATCH_SIZE)
            .wait_time_seconds(self.poll_secs as i32)
            // I think this should be a known attribute
            // https://github.com/awslabs/aws-sdk-rust/issues/411
            .attribute_names(QueueAttributeName::Unknown(String::from("SentTimestamp")))
            .send()
            .await;

        let receive_message_output = match result {
            Ok(output) => output,
            Err(err) => {
                error!("SQS receive message error: {:?}", err);
                // prevent rapid errors from flooding the logs
                tokio::time::sleep(Duration::from_secs(1)).await;
                return;
            }
        };

        if let Some(messages) = receive_message_output.messages {
            let mut receipts_to_ack = vec![];

            let mut batch_receiver = None;
            for message in messages {
                if let Some(body) = message.body {
                    emit!(&AwsSqsBytesReceived {
                        byte_size: body.len()
                    });
                    let timestamp = get_timestamp(&message.attributes);
                    let stream = decode_message(self.decoder.clone(), &body, timestamp);
                    pin!(stream);
                    let send_result = if acknowledgements {
                        let (batch, receiver) = BatchNotifier::new_with_receiver();
                        let mut stream = stream.map(|event| event.with_batch_notifier(&batch));
                        batch_receiver = Some(receiver);
                        out.send_all(&mut stream).await
                    } else {
                        out.send_all(&mut stream).await
                    };

                    match send_result {
                        Err(err) => error!(message = "Error sending to sink.", error = %err),
                        Ok(()) => {
                            // a receipt handle should always exist
                            if let Some(receipt_handle) = message.receipt_handle {
                                receipts_to_ack.push(receipt_handle);
                            }
                        }
                    }
                }
            }

            if let Some(receiver) = batch_receiver {
                let client = self.client.clone();
                let queue_url = self.queue_url.clone();
                tokio::spawn(async move {
                    let batch_status = receiver.await;
                    if batch_status == BatchStatus::Delivered {
                        delete_messages(&client, &receipts_to_ack, &queue_url).await;
                    }
                });
            } else {
                delete_messages(&self.client, &receipts_to_ack, &self.queue_url).await;
            }
        }
    }
}

fn get_timestamp(
    attributes: &Option<HashMap<MessageSystemAttributeName, String>>,
) -> Option<DateTime<Utc>> {
    attributes.as_ref().and_then(|attributes| {
        let sent_time_str = attributes.get(&MessageSystemAttributeName::SentTimestamp)?;
        Some(Utc.timestamp_millis(i64::from_str(sent_time_str).ok()?))
    })
}

async fn delete_messages(client: &SqsClient, receipts: &[String], queue_url: &str) {
    if !receipts.is_empty() {
        let mut batch = client.delete_message_batch().queue_url(queue_url);

        for (id, receipt) in receipts.iter().enumerate() {
            batch = batch.entries(
                DeleteMessageBatchRequestEntry::builder()
                    .id(id.to_string())
                    .receipt_handle(receipt)
                    .build(),
            );
        }
        if let Err(err) = batch.send().await {
            emit!(&SqsMessageDeleteError { error: &err });
        }
    }
}

fn decode_message(
    decoder: Decoder,
    message: &str,
    sent_time: Option<DateTime<Utc>>,
) -> impl Stream<Item = Event> {
    let schema = log_schema();

    let payload = Cursor::new(Bytes::copy_from_slice(message.as_bytes()));
    let mut stream = FramedRead::new(payload, decoder);

    stream! {
        loop {
            match stream.next().await {
                Some(Ok((events, _))) => {
                    let count = events.len();
                    let mut total_events_size = 0;
                    for mut event in events {
                        if let Event::Log(ref mut log) = event {
                            log.try_insert(schema.source_type_key(), Bytes::from("aws_sqs"));
                            if let Some(sent_time) = sent_time {
                                log.try_insert(schema.timestamp_key(), sent_time);
                            }
                        }
                        total_events_size += event.size_of();
                        yield event;
                    }
                    emit!(&EventsReceived {
                        byte_size: total_events_size,
                        count
                    });
                },
                Some(Err(error)) => {
                    // Error is logged by `crate::codecs::Decoder`, no further handling
                    // is needed here.
                    if !error.can_continue() {
                        break;
                    }
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::SecondsFormat;

    use super::*;

    #[tokio::test]
    async fn test_decode() {
        let message = "test";
        let now = Utc::now();
        let stream = decode_message(Decoder::default(), "test", Some(now));
        let events: Vec<_> = stream.collect().await;
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0]
                .clone()
                .as_log()
                .get(log_schema().message_key())
                .unwrap()
                .to_string_lossy(),
            message
        );
        assert_eq!(
            events[0]
                .clone()
                .as_log()
                .get(log_schema().timestamp_key())
                .unwrap()
                .to_string_lossy(),
            now.to_rfc3339_opts(SecondsFormat::AutoSi, true)
        );
    }

    #[test]
    fn test_get_timestamp() {
        let attributes = HashMap::from([(
            MessageSystemAttributeName::SentTimestamp,
            "1636408546018".to_string(),
        )]);

        assert_eq!(
            get_timestamp(&Some(attributes)),
            Some(Utc.timestamp_millis(1636408546018))
        );
    }
}
