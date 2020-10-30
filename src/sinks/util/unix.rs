use crate::{
    config::SinkContext,
    internal_events::{
        ConnectionOpen, OpenGauge, OpenTokenDyn, UnixSocketConnectionEstablished,
        UnixSocketConnectionFailure, UnixSocketEventSent, UnixSocketFlushFailed,
        UnixSocketSendFailed,
    },
    sinks::util::StreamSinkOld,
    sinks::{Healthcheck, VectorSink},
    Event,
};
use bytes::Bytes;
use futures::{compat::CompatSink, future::BoxFuture, FutureExt, TryFutureExt};
use futures01::{stream, try_ready, Async, AsyncSink, Future, Poll as Poll01, Sink, StartSend};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{path::PathBuf, time::Duration};
use tokio::{
    net::UnixStream,
    time::{delay_for, Delay},
};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_util::codec::{BytesCodec, FramedWrite};

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

    pub fn build<F>(
        &self,
        cx: SinkContext,
        encode_event: F,
    ) -> crate::Result<(VectorSink, Healthcheck)>
    where
        F: Fn(Event) -> Option<Bytes> + Send + 'static,
    {
        let (connector, healthcheck) = self.build_connector()?;
        let sink: UnixSink = connector.into();
        let sink = StreamSinkOld::new(sink, cx.acker())
            .with_flat_map(move |event| stream::iter_ok(encode_event(event)));

        Ok((VectorSink::Futures01Sink(Box::new(sink)), healthcheck))
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

    fn connect(&self) -> BoxFuture<'static, Result<UnixSocket, UnixSocketError>> {
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

impl Into<UnixSink> for UnixConnector {
    fn into(self) -> UnixSink {
        UnixSink::new(self.path)
    }
}

#[derive(Debug, Snafu)]
pub enum UnixSocketError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: tokio::io::Error },
    #[snafu(display("Send error: {}", source))]
    SendError { source: tokio::io::Error },
}

pub struct UnixSink {
    connector: UnixConnector,
    state: UnixSinkState,
    backoff: ExponentialBackoff,
}

type UnixSocket = FramedWrite<UnixStream, BytesCodec>;
type UnixSocket01 = CompatSink<UnixSocket, Bytes>;

enum UnixSinkState {
    Disconnected,
    Creating(Box<dyn Future<Item = UnixSocket, Error = UnixSocketError> + Send>),
    Open(UnixSocket01, OpenTokenDyn),
    Backoff(Box<dyn Future<Item = (), Error = ()> + Send>),
}

impl UnixSink {
    pub fn new(path: PathBuf) -> Self {
        let connector = UnixConnector { path };
        Self {
            connector,
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

    fn next_delay01(&mut self) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        let delay = self.next_delay();
        Box::new(async move { Ok(delay.await) }.boxed().compat())
    }

    /**
     * Polls for whether the underlying UnixStream is connected and ready to receive writes.
     **/
    fn poll_connection(&mut self) -> Poll01<&mut UnixSocket01, ()> {
        loop {
            self.state = match self.state {
                UnixSinkState::Open(ref mut stream, _) => return Ok(Async::Ready(stream)),
                UnixSinkState::Creating(ref mut connect_future) => match connect_future.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(error) => {
                        emit!(UnixSocketConnectionFailure {
                            error,
                            path: &self.connector.path,
                        });
                        UnixSinkState::Backoff(self.next_delay01())
                    }
                    Ok(Async::Ready(socket)) => {
                        emit!(UnixSocketConnectionEstablished {
                            path: &self.connector.path,
                        });
                        self.backoff = Self::fresh_backoff();
                        UnixSinkState::Open(
                            CompatSink::new(socket),
                            OpenGauge::new()
                                .open(Box::new(|count| emit!(ConnectionOpen { count }))),
                        )
                    }
                },
                UnixSinkState::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(()) => unreachable!(),
                    Ok(Async::Ready(())) => UnixSinkState::Disconnected,
                },
                UnixSinkState::Disconnected => {
                    debug!(
                        message = "Connecting.",
                        path = %self.connector.path.to_str().unwrap()
                    );
                    let fut = self.connector.connect();
                    UnixSinkState::Creating(Box::new(fut.compat()))
                }
            }
        }
    }
}

impl Sink for UnixSink {
    type SinkItem = Bytes;
    type SinkError = ();

    fn start_send(&mut self, line: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let byte_size = line.len();
        match self.poll_connection() {
            Ok(Async::Ready(connection)) => match connection.start_send(line) {
                Err(error) => {
                    emit!(UnixSocketSendFailed {
                        error,
                        path: &self.connector.path,
                    });
                    self.state = UnixSinkState::Disconnected;
                    Ok(AsyncSink::Ready)
                }
                Ok(res) => {
                    emit!(UnixSocketEventSent { byte_size });
                    Ok(res)
                }
            },
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(line)),
            Err(_) => unreachable!(),
        }
    }

    fn poll_complete(&mut self) -> Poll01<(), Self::SinkError> {
        // Stream::forward will immediately poll_complete the sink it's forwarding to,
        // but we don't want to connect before the first event actually comes through.
        if let UnixSinkState::Disconnected = self.state {
            return Ok(Async::Ready(()));
        }

        let connection = try_ready!(self.poll_connection());

        match connection.poll_complete() {
            Err(error) => {
                emit!(UnixSocketFlushFailed {
                    error,
                    path: &self.connector.path,
                });
                self.state = UnixSinkState::Disconnected;
                Ok(Async::Ready(()))
            }
            Ok(res) => Ok(res),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::{encode_event, Encoding};
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
        let encoding = Encoding::Text.into();
        let (sink, _healthcheck) = config
            .build(cx, move |event| encode_event(event, &encoding))
            .unwrap();

        // Send the test data
        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        sink.run(events).await.unwrap();

        // Wait for output to connect
        receiver.connected().await;

        // Receive the data sent by the Sink to the receiver
        assert_eq!(input_lines, receiver.await);
    }
}
