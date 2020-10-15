use crate::{
    buffers::Acker,
    config::SinkContext,
    internal_events::{
        UnixSocketConnectionEstablished,
        UnixSocketConnectionFailure,
        UnixSocketError,
        UnixSocketEventSent,
        // UnixSocketFlushFailed, UnixSocketSendFailed,
    },
    sinks::util::{encode_event, encoding::EncodingConfig, Encoding, StreamSink},
    sinks::{Healthcheck, VectorSink},
    Event,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{future::BoxFuture, stream::BoxStream, FutureExt, SinkExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::task::{Context, Poll};
use std::{path::PathBuf, time::Duration};
use tokio::{
    net::UnixStream,
    time::{delay_for, Delay},
};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_util::codec::{BytesCodec, FramedWrite};

#[derive(Debug, Snafu)]
pub enum UnixStreamError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: tokio::io::Error },
    #[snafu(display("Send error: {}", source))]
    SendError { source: tokio::io::Error },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UnixSinkConfig {
    pub path: PathBuf,
}

impl UnixSinkConfig {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn build_connector(&self) -> crate::Result<(UnixConnector, Healthcheck)> {
        let connector = UnixConnector::new(self.path.clone());
        let healthcheck = connector.healthcheck().boxed();
        Ok((connector, healthcheck))
    }

    pub fn build_service(&self) -> crate::Result<(UnixService, Healthcheck)> {
        let (connector, healthcheck) = self.build_connector()?;
        Ok((connector.into(), healthcheck))
    }

    pub fn build(
        &self,
        cx: SinkContext,
        encoding: EncodingConfig<Encoding>,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let (connector, healthcheck) = self.build_connector()?;
        let sink = UnixSink::new(connector.path, cx.acker(), encoding);
        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }
}

#[derive(Clone)]
struct UnixConnector {
    path: PathBuf,
}

impl UnixConnector {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn connect(
        &self,
    ) -> BoxFuture<'static, Result<FramedWrite<UnixStream, BytesCodec>, UnixStreamError>> {
        let path = self.path.clone();

        async move {
            let stream = UnixStream::connect(&path).await.context(ConnectError)?;
            Ok(FramedWrite::new(stream, BytesCodec::new()))
        }
        .boxed()
    }

    fn healthcheck(&self) -> BoxFuture<'static, crate::Result<()>> {
        self.connect().map_ok(|_| ()).map_err(|e| e.into()).boxed()
    }
}

impl From<UnixConnector> for UnixService {
    fn from(connector: UnixConnector) -> UnixService {
        UnixService { connector }
    }
}

pub struct UnixService {
    connector: UnixConnector,
}

impl tower::Service<Bytes> for UnixService {
    type Response = ();
    type Error = UnixStreamError;
    type Future = BoxFuture<'static, Result<(), Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: Bytes) -> Self::Future {
        let connect = self.connector.connect();
        async move { connect.await?.send(msg).await.context(SendError) }.boxed()
    }
}

enum UnixSinkState {
    Connected(FramedWrite<UnixStream, BytesCodec>),
    Disconnected,
}

pub struct UnixSink {
    path: PathBuf,
    acker: Acker,
    encoding: EncodingConfig<Encoding>,
    state: UnixSinkState,
    backoff: ExponentialBackoff,
}

impl UnixSink {
    pub fn new(path: PathBuf, acker: Acker, encoding: EncodingConfig<Encoding>) -> Self {
        Self {
            path,
            acker,
            encoding,
            state: UnixSinkState::Disconnected,
            backoff: Self::fresh_backoff(),
        }
    }

    fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    fn next_delay(&mut self) -> Delay {
        delay_for(self.backoff.next().unwrap())
    }

    async fn get_stream(&mut self) -> &mut FramedWrite<UnixStream, BytesCodec> {
        loop {
            match self.state {
                UnixSinkState::Connected(ref mut stream) => return stream,
                UnixSinkState::Disconnected => {
                    debug!(
                        message = "Connecting",
                        path = %self.path.to_str().unwrap()
                    );
                    match UnixStream::connect(self.path.clone()).await {
                        Ok(stream) => {
                            emit!(UnixSocketConnectionEstablished { path: &self.path });
                            let out = FramedWrite::new(stream, BytesCodec::new());
                            self.state = UnixSinkState::Connected(out);
                            self.backoff = Self::fresh_backoff()
                        }
                        Err(error) => {
                            emit!(UnixSocketConnectionFailure {
                                error,
                                path: &self.path
                            });
                            self.next_delay().await
                        }
                    }
                }
            }
        }
    }
}

#[async_trait]
impl StreamSink for UnixSink {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        // use futures::future;
        // let encoding = self.encoding.clone();
        // let encode_event = move |event| encode_event(event, &encoding);

        // let mut slot = None;
        // loop {
        //     let stream = self.get_stream().await;

        //     let mut stream_error = None;
        //     tokio::select! {
        //         // poll_ready => start_send
        //         // poll_flush => ack
        //         // next => slot
        //         event = input.next(), if slot.is_none() => match event {
        //             Some(event) => slot = encode_event(event),
        //             None => break,
        //         },
        //         ready = future::poll_fn(|cx| stream.poll_ready_unpin(cx)), if slot.is_some() => match ready {
        //             Ok(()) => match stream.start_send(slot.take().expect("slot should not be empty")) {

        //             },
        //             Err(error) => {
        //                 stream_error = Some(error);
        //                 // emit!(UnixSocketError { error, path: &self.path });
        //                 // self.state = UnixSinkState::Disconnected;
        //             }
        //         },
        //     };
        // }

        // TODO: use select! for `input.next()` & `stream.start_send / stream.poll_flush`.
        while let Some(event) = input.next().await {
            if let Some(bytes) = encode_event(event, &self.encoding) {
                let stream = self.get_stream().await;

                let byte_size = bytes.len();
                match stream.send(bytes).await {
                    Ok(()) => emit!(UnixSocketEventSent { byte_size }),
                    Err(error) => {
                        emit!(UnixSocketError {
                            error,
                            path: &self.path
                        });
                        self.state = UnixSinkState::Disconnected;
                    }
                };
            }

            self.acker.ack(1);
        }

        if let UnixSinkState::Connected(stream) = &mut self.state {
            if let Err(error) = stream.close().await {
                emit!(UnixSocketError {
                    error,
                    path: &self.path
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{random_lines_with_stream, CountReceiver};
    use tokio::net::UnixListener;

    fn temp_uds_path(name: &str) -> PathBuf {
        tempfile::tempdir().unwrap().into_path().join(name)
    }

    #[tokio::test]
    async fn unix_sink_healthcheck() {
        let good_path = temp_uds_path("valid_uds");
        let _listener = UnixListener::bind(&good_path).unwrap();
        assert!(UnixConnector::new(good_path).healthcheck().await.is_ok());

        let bad_path = temp_uds_path("no_one_listening");
        assert!(UnixConnector::new(bad_path).healthcheck().await.is_err());
    }

    #[tokio::test]
    async fn basic_unix_sink() {
        let num_lines = 1000;
        let out_path = temp_uds_path("unix_test");

        // Set up server to receive events from the Sink.
        let mut receiver = CountReceiver::receive_lines_unix(out_path.clone());

        // Set up Sink
        let config = UnixSinkConfig::new(out_path);
        let cx = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(cx, Encoding::Text.into()).unwrap();

        // Send the test data
        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        sink.run(events).await.unwrap();

        // Wait for output to connect
        receiver.connected().await;

        // Receive the data sent by the Sink to the receiver
        assert_eq!(input_lines, receiver.await);
    }
}
