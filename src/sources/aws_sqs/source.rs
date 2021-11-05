use crate::codecs::Decoder;
use crate::config::log_schema;
use crate::event::{BatchNotifier, Event};
use crate::event::{BatchStatus};
use crate::shutdown::ShutdownSignal;
use crate::sources::util::TcpError;
use crate::Pipeline;
use aws_sdk_sqs::model::DeleteMessageBatchRequestEntry;

use aws_sdk_sqs::Client as SqsClient;
use bytes::Bytes;
use futures::future::ready;
use futures::{FutureExt, TryStreamExt};
use futures::{SinkExt, Stream, StreamExt};
use std::io::Cursor;
use std::panic;
use tokio::time::Duration;
use tokio::{pin, select};
use tokio_util::codec::FramedRead;

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
    pub async fn run(self, out: Pipeline, shutdown: ShutdownSignal) -> Result<(), ()> {
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

    async fn run_once(&self, out: &mut Pipeline, acknowledgements: bool) {
        let result = self
            .client
            .receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(MAX_BATCH_SIZE)
            .wait_time_seconds(self.poll_secs as i32)
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
            if acknowledgements {
                let (batch, receiver) = BatchNotifier::new_with_receiver();
                for message in messages {
                    if let Some(body) = message.body {
                        let stream = decode_message(self.decoder.clone(), &body);

                        let mut stream = stream.map_ok(|event| event.with_batch_notifier(&batch));
                        let send_result = out.send_all(&mut stream).await;

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

                let client = self.client.clone();
                let queue_url = self.queue_url.clone();
                tokio::spawn(async move {
                    let batch_status = receiver.await;
                    if batch_status == BatchStatus::Delivered {
                        delete_messages(&client, &receipts_to_ack, &queue_url).await;
                    }
                });
            } else {
                for message in messages {
                    if let Some(body) = message.body {
                        let mut stream = decode_message(self.decoder.clone(), &body);
                        match out.send_all(&mut stream).await {
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
                delete_messages(&self.client, &receipts_to_ack, &self.queue_url).await;
            }
        }
    }
}

async fn delete_messages(client: &SqsClient, receipts: &[String], queue_url: &str) {
    if !receipts.is_empty() {
        let mut batch = client.delete_message_batch().queue_url(queue_url);

        for (id, receipt) in receipts.into_iter().enumerate() {
            batch = batch.entries(
                DeleteMessageBatchRequestEntry::builder()
                    .id(id.to_string())
                    .receipt_handle(receipt)
                    .build(),
            );
        }
        if let Err(err) = batch.send().await {
            //TODO: emit as event?
            error!("SQS Delete failed: {:?}", err);
        }
    }
}

fn decode_message<E>(decoder: Decoder, message: &str) -> impl Stream<Item = Result<Event, E>> {
    let schema = log_schema();

    let payload = Cursor::new(Bytes::copy_from_slice(message.as_bytes()));
    FramedRead::new(payload, decoder)
        .map(|input| match input {
            Ok((mut events, _)) => {
                let mut event = events.pop().expect("event must exist");
                if let Event::Log(ref mut log) = event {
                    log.try_insert(schema.source_type_key(), Bytes::from("aws_sqs"));
                    // log.try_insert(schema.timestamp_key(), timestamp);
                }

                Some(Some(Ok(event)))
            }
            Err(e) => {
                // Error is logged by `crate::codecs::Decoder`, no further handling
                // is needed here.
                if !e.can_continue() {
                    Some(None)
                } else {
                    None
                }
            }
        })
        .take_while(|x| ready(x.is_some()))
        .filter_map(|x| ready(x.expect("should have inner value")))
}
