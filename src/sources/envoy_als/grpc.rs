use crate::{internal_events::StreamClosedError, shutdown::ShutdownSignal, SourceSender};
use envoy_proto::envoy::service::accesslog::v3::{
    access_log_service_server::AccessLogService, stream_access_logs_message::Identifier,
    stream_access_logs_message::LogEntries, StreamAccessLogsMessage, StreamAccessLogsResponse,
};
use tonic::{Request, Response, Status, Streaming};
use value::Value;
use vector_common::internal_event::{
    ByteSize, BytesReceived, CountByteSize, EventsReceived, InternalEventHandle as _, Registered,
};
use vector_core::{event::LogEvent, EstimatedJsonEncodedSizeOf};

#[derive(Clone)]
pub(super) struct Service {
    pub events_received: Registered<EventsReceived>,
    pub bytes_received: Registered<BytesReceived>,
    pub pipeline: SourceSender,
    pub shutdown: ShutdownSignal,
}

enum StreamError {
    Grpc(Status),
    StreamClosed,
}

#[tonic::async_trait]
impl AccessLogService for Service {
    async fn stream_access_logs(
        &self,
        request: Request<Streaming<StreamAccessLogsMessage>>,
    ) -> Result<Response<StreamAccessLogsResponse>, Status> {
        let mut in_stream = request.into_inner();
        let mut shutdown = self.shutdown.clone();
        let mut stream_identifier = None;

        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                stream_msg = in_stream.message() => {
                    match self.handle_message(stream_msg, &mut stream_identifier).await {
                        Ok(_) => {},
                        Err(err) => {
                            match err {
                                StreamError::StreamClosed => break,
                                StreamError::Grpc(status) => {
                                    // TODO emit error?
                                    return Err(status);
                                }
                            }
                        }
                    }
                },
            }
        }

        Ok(Response::new(StreamAccessLogsResponse {}))
    }
}

impl Service {
    async fn handle_message(
        &self,
        stream_msg: Result<Option<StreamAccessLogsMessage>, Status>,
        stream_identifier: &mut Option<Identifier>,
    ) -> Result<(), StreamError> {
        match stream_msg {
            Ok(omsg) => match omsg {
                Some(msg) => {
                    self.process_logs(msg, stream_identifier).await?;
                }
                None => {
                    debug!("Sender closed stream");
                    return Err(StreamError::StreamClosed);
                }
            },
            Err(err) => {
                return Err(StreamError::Grpc(err));
            }
        }
        Ok(())
    }

    async fn process_logs(
        &self,
        msg: StreamAccessLogsMessage,
        stream_identifier: &mut Option<Identifier>,
    ) -> Result<(), StreamError> {
        let mut events = vec![];

        // identifier is only sent on the first message of a stream,
        // but we want to have it alongside all log events so they can be
        // identified by the Envoy instance that sent them. To do this,
        // we store the identifier sent on the stream and tack it on to
        // future events.
        if let Some(identifier) = msg.identifier {
            *stream_identifier = Some(identifier);
        }

        if let Some(logs_entries) = msg.log_entries {
            match logs_entries {
                LogEntries::HttpLogs(http_logs) => {
                    for l in http_logs.log_entry {
                        let mut evt = LogEvent::default();
                        evt.insert("http_log", Value::from(l));

                        if let Some(stream_id) = stream_identifier.clone() {
                            evt.insert("identifier", Value::from(stream_id));
                        }

                        events.push(evt);
                    }
                }
                LogEntries::TcpLogs(_tcp_logs) => {
                    warn!("Received TCP log entry. TCP logs are not yet supported by Vector.");
                    return Ok(());
                }
            }
        }

        let count = events.len();
        let byte_size = events.estimated_json_encoded_size_of();
        self.events_received.emit(CountByteSize(count, byte_size));
        self.bytes_received.emit(ByteSize(byte_size));

        let resp = self.pipeline.clone().send_batch(events).await;
        match resp {
            Ok(()) => {}
            Err(error) => {
                let message = error.to_string();
                emit!(StreamClosedError { error, count });
                return Err(StreamError::Grpc(Status::unavailable(message)));
            }
        }
        Ok(())
    }
}
