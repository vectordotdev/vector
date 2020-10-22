use crate::{
    buffers::Acker,
    config::SinkContext,
    internal_events::{
        UnixSocketConnectionEstablished, UnixSocketConnectionFailure, UnixSocketError,
        UnixSocketEventSent,
    },
    sinks::util::{
        acker_bytes_sink::AckerBytesSink, encode_event, encoding::EncodingConfig, Encoding,
        StreamSink,
    },
    sinks::{Healthcheck, VectorSink},
    Event,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{future::BoxFuture, stream::BoxStream, FutureExt, SinkExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    path::PathBuf,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{net::UnixStream, time::delay_for};
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

pub struct UnixSink {
    path: PathBuf,
    acker: Acker,
    encoding: EncodingConfig<Encoding>,
}

impl UnixSink {
    pub fn new(path: PathBuf, acker: Acker, encoding: EncodingConfig<Encoding>) -> Self {
        Self {
            path,
            acker,
            encoding,
        }
    }

    fn fresh_backoff() -> ExponentialBackoff {
        // TODO: make configurable
        ExponentialBackoff::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }

    async fn connect(&mut self) -> AckerBytesSink<UnixStream> {
        let mut backoff = Self::fresh_backoff();
        loop {
            debug!(
                message = "Connecting",
                path = %self.path.to_str().unwrap()
            );
            match UnixStream::connect(self.path.clone()).await {
                Ok(stream) => {
                    emit!(UnixSocketConnectionEstablished { path: &self.path });
                    return AckerBytesSink::new(
                        stream,
                        self.acker.clone(),
                        Box::new(|byte_size| emit!(UnixSocketEventSent { byte_size })),
                    );
                }
                Err(error) => {
                    emit!(UnixSocketConnectionFailure {
                        error,
                        path: &self.path
                    });
                    delay_for(backoff.next().unwrap()).await;
                }
            }
        }
    }
}

#[async_trait]
impl StreamSink for UnixSink {
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let encoding = self.encoding.clone();
        let mut input = input
            // We send event empty events because `AckerBytesSink` `ack` and `emit!` for us.
            .map(|event| match encode_event(event, &encoding) {
                Some(bytes) => bytes,
                None => Bytes::new(),
            })
            .map(Ok)
            .peekable();

        while Pin::new(&mut input).peek().await.is_some() {
            let mut sink = self.connect().await;
            if let Err(error) = sink.send_all(&mut input).await {
                emit!(UnixSocketError {
                    error,
                    path: &self.path
                });
            }
            sink.ack();
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
