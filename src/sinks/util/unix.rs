use crate::{
    internal_events::{
        UnixSocketConnectionEstablished, UnixSocketConnectionFailure, UnixSocketError,
        UnixSocketEventSent,
    },
    sinks::util::{encode_event, encoding::EncodingConfig, Encoding, StreamSink},
    sinks::{Healthcheck, RouterSink},
    topology::config::SinkContext,
};
use bytes05::Bytes;
use futures::{compat::CompatSink, FutureExt, TryFutureExt};
use futures01::{stream::iter_ok, try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{path::PathBuf, time::Duration};
use tokio::{
    net::UnixStream,
    time::{delay_for, Delay},
};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_util::codec::{BytesCodec, FramedWrite};
use tracing::field;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UnixSinkConfig {
    pub path: PathBuf,
    pub encoding: EncodingConfig<Encoding>,
}

impl UnixSinkConfig {
    pub fn new(path: PathBuf, encoding: EncodingConfig<Encoding>) -> Self {
        Self { path, encoding }
    }

    pub fn build(&self, cx: SinkContext) -> crate::Result<(RouterSink, Healthcheck)> {
        let encoding = self.encoding.clone();
        let unix = UnixSink::new(self.path.clone());
        let sink = StreamSink::new(unix, cx.acker());

        let sink =
            Box::new(sink.with_flat_map(move |event| iter_ok(encode_event(event, &encoding))));
        let healthcheck = healthcheck(self.path.clone()).boxed().compat();

        Ok((sink, Box::new(healthcheck)))
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: tokio::io::Error },
}

async fn healthcheck(path: PathBuf) -> crate::Result<()> {
    match UnixStream::connect(&path).await {
        Ok(_) => Ok(()),
        Err(source) => Err(HealthcheckError::ConnectError { source }.into()),
    }
}

pub struct UnixSink {
    path: PathBuf,
    state: UnixSinkState,
    backoff: ExponentialBackoff,
}

enum UnixSinkState {
    Disconnected,
    Creating(Box<dyn Future<Item = UnixStream, Error = tokio::io::Error> + Send + 'static>),
    Open(CompatSink<FramedWrite<UnixStream, BytesCodec>, Bytes>),
    Backoff(Box<dyn Future<Item = (), Error = ()> + Send>),
}

impl UnixSink {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
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

    /**
     * Polls for whether the underlying UnixStream is connected and ready to receive writes.
     **/
    fn poll_connection(
        &mut self,
    ) -> Poll<&mut CompatSink<FramedWrite<UnixStream, BytesCodec>, Bytes>, ()> {
        loop {
            self.state = match self.state {
                UnixSinkState::Open(ref mut stream) => {
                    return Ok(Async::Ready(stream));
                }
                UnixSinkState::Creating(ref mut connect_future) => match connect_future.poll() {
                    Ok(Async::NotReady) => {
                        return Ok(Async::NotReady);
                    }
                    Err(error) => {
                        emit!(UnixSocketConnectionFailure {
                            error,
                            path: &self.path
                        });
                        let delay = self.next_delay();
                        let delay = Box::new(async move { Ok(delay.await) }.boxed().compat());
                        UnixSinkState::Backoff(delay)
                    }
                    Ok(Async::Ready(stream)) => {
                        emit!(UnixSocketConnectionEstablished { path: &self.path });
                        self.backoff = Self::fresh_backoff();
                        let out = FramedWrite::new(stream, BytesCodec::new());
                        UnixSinkState::Open(CompatSink::new(out))
                    }
                },
                UnixSinkState::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(()) => unreachable!(),
                    Ok(Async::Ready(())) => UnixSinkState::Disconnected,
                },
                UnixSinkState::Disconnected => {
                    debug!(
                        message = "connecting",
                        path = &field::display(self.path.to_str().unwrap())
                    );
                    let connect_future = UnixStream::connect(self.path.clone()).boxed().compat();
                    UnixSinkState::Creating(Box::new(connect_future))
                }
            }
        }
    }
}

impl Sink for UnixSink {
    type SinkItem = bytes::Bytes;
    type SinkError = ();

    fn start_send(&mut self, line: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let byte_size = line.len();
        match self.poll_connection() {
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(line)),
            Err(_) => {
                unreachable!(); // poll_ready() should never return an error
            }
            Ok(Async::Ready(connection)) => {
                let line = Bytes::copy_from_slice(&line);
                match connection.start_send(line) {
                    Err(error) => {
                        emit!(UnixSocketError {
                            error,
                            path: &self.path
                        });
                        self.state = UnixSinkState::Disconnected;
                        Ok(AsyncSink::Ready)
                    }
                    Ok(res) => {
                        emit!(UnixSocketEventSent { byte_size });
                        Ok(match res {
                            AsyncSink::Ready => AsyncSink::Ready,
                            AsyncSink::NotReady(bytes) => {
                                let bytes = bytes::Bytes::from(&bytes[..]);
                                AsyncSink::NotReady(bytes)
                            }
                        })
                    }
                }
            }
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        // Stream::forward will immediately poll_complete the sink it's forwarding to,
        // but we don't want to connect before the first event actually comes through.
        if let UnixSinkState::Disconnected = self.state {
            return Ok(Async::Ready(()));
        }

        let connection = try_ready!(self.poll_connection());

        match connection.poll_complete() {
            Err(error) => {
                emit!(UnixSocketError {
                    error,
                    path: &self.path
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
    use crate::test_util::{random_lines_with_stream, CountReceiver};
    use futures::compat::Future01CompatExt;
    use futures01::Sink;
    use tokio::net::UnixListener;

    fn temp_uds_path(name: &str) -> PathBuf {
        tempfile::tempdir().unwrap().into_path().join(name)
    }

    #[tokio::test]
    async fn unix_sink_healthcheck() {
        let good_path = temp_uds_path("valid_uds");
        let _listener = UnixListener::bind(&good_path).unwrap();
        assert!(healthcheck(good_path).await.is_ok());

        let bad_path = temp_uds_path("no_one_listening");
        assert!(healthcheck(bad_path).await.is_err());
    }

    #[tokio::test]
    async fn basic_unix_sink() {
        let num_lines = 1000;
        let out_path = temp_uds_path("unix_test");

        // Set up server to receive events from the Sink.
        let mut receiver = CountReceiver::receive_lines_unix(out_path.clone());

        // Set up Sink
        let config = UnixSinkConfig::new(out_path, Encoding::Text.into());
        let cx = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(cx).unwrap();

        // Send the test data
        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let _ = sink.send_all(events).compat().await.unwrap();

        // Wait for output to connect
        receiver.connected().await;

        // Receive the data sent by the Sink to the receiver
        assert_eq!(input_lines, receiver.wait().await);
    }
}
