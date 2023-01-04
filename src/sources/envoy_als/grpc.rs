use crate::{
    internal_events::{EventsReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    SourceSender,
};
use envoy_proto::envoy::service::accesslog::v3::{
    access_log_service_server::AccessLogService, stream_access_logs_message::LogEntries,
    StreamAccessLogsMessage, StreamAccessLogsResponse,
};
use tonic::{Request, Response, Status, Streaming};
use value::Value;
use vector_common::internal_event::{CountByteSize, InternalEventHandle as _, Registered};
use vector_core::{event::LogEvent, EstimatedJsonEncodedSizeOf};

#[derive(Clone)]
pub(super) struct Service {
    pub events_received: Registered<EventsReceived>,
    pub pipeline: SourceSender,
    pub shutdown: ShutdownSignal,
}

#[tonic::async_trait]
impl AccessLogService for Service {
    async fn stream_access_logs(
        &self,
        request: Request<Streaming<StreamAccessLogsMessage>>,
    ) -> Result<Response<StreamAccessLogsResponse>, Status> {
        let mut in_stream = request.into_inner();
        let mut shutdown = self.shutdown.clone();

        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                stream_msg = in_stream.message() => {
                    match stream_msg {
                        Ok(omsg) => {
                            match omsg {
                                Some(msg) => {
                                    let mut events = vec![];

                                    if let Some(logs_entries) = msg.log_entries {
                                        match logs_entries {
                                            LogEntries::HttpLogs(http_logs) => {
                                                for l in http_logs.log_entry {
                                                    let mut evt = LogEvent::default();
                                                    evt.insert("http_log", Value::from(l));

                                                    if let Some(identifier) = msg.identifier.clone() {
                                                        evt.insert("identifier", Value::from(identifier));
                                                    }

                                                    events.push(evt);
                                                }
                                            },
                                            LogEntries::TcpLogs(_tcp_logs) => {
                                                warn!("TCP logs are unsupported at this time.");
                                                continue;
                                            }
                                        }
                                    }

                                    let count = events.len();
                                    let byte_size = events.estimated_json_encoded_size_of();
                                    self.events_received.emit(CountByteSize(count, byte_size));

                                    let resp = self.pipeline
                                        .clone()
                                        .send_batch(events)
                                        .await;
                                    match resp {
                                        Ok(()) => {},
                                        Err(error) => {
                                            let message = error.to_string();
                                            emit!(StreamClosedError { error, count });
                                            return Err(Status::unavailable(message));
                                        }
                                    }
                                },
                                None => {
                                    debug!("Sender closed stream");
                                    break;
                                }
                            }
                        },
                        Err(err) => {
                            warn!("error getting stream msg: {}", err);
                            break;
                        }
                    }
                },
            }
        }

        Ok(Response::new(StreamAccessLogsResponse {}))
    }
}
