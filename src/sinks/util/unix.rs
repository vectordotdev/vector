use crate::{
    internal_events::{
        UnixSocketConnectionEstablished, UnixSocketConnectionFailure, UnixSocketError,
        UnixSocketEventSent,
    },
    sinks::util::{encode_event, encoding::EncodingConfig, Encoding, StreamSink},
    sinks::{Healthcheck, RouterSink},
    topology::config::SinkContext,
};
use bytes::Bytes;
use futures01::{
    future, stream::iter_ok, try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio01::codec::{BytesCodec, FramedWrite};
use tokio01::timer::Delay;
use tokio_retry::strategy::ExponentialBackoff;
use tokio_uds::UnixStream;
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
        let healthcheck = unix_healthcheck(self.path.clone());

        Ok((sink, healthcheck))
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
}

fn unix_healthcheck(path: PathBuf) -> Healthcheck {
    // Lazy to avoid immediately connecting
    let check = future::lazy(move || {
        UnixStream::connect(&path)
            .map(|_| ())
            .map_err(|source| HealthcheckError::ConnectError { source }.into())
    });

    Box::new(check)
}

pub struct UnixSink {
    path: PathBuf,
    state: UnixSinkState,
    backoff: ExponentialBackoff,
}

enum UnixSinkState {
    Disconnected,
    Creating(Box<dyn Future<Item = UnixStream, Error = io::Error> + Send + 'static>),
    Open(FramedWrite<UnixStream, BytesCodec>),
    Backoff(Delay),
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
        Delay::new(Instant::now() + self.backoff.next().unwrap())
    }

    /**
     * Polls for whether the underlying UnixStream is connected and ready to receive writes.
     **/
    fn poll_connection(&mut self) -> Poll<&mut FramedWrite<UnixStream, BytesCodec>, ()> {
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
                        UnixSinkState::Backoff(self.next_delay())
                    }
                    Ok(Async::Ready(stream)) => {
                        emit!(UnixSocketConnectionEstablished { path: &self.path });
                        self.backoff = Self::fresh_backoff();
                        let out = FramedWrite::new(stream, BytesCodec::new());
                        UnixSinkState::Open(out)
                    }
                },
                UnixSinkState::Backoff(ref mut delay) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    // Err can only occur if the tokio runtime has been shutdown or if more than 2^63 timers have been created
                    Err(err) => unreachable!(err),
                    Ok(Async::Ready(())) => UnixSinkState::Disconnected,
                },
                UnixSinkState::Disconnected => {
                    debug!(
                        message = "connecting",
                        path = &field::display(self.path.to_str().unwrap())
                    );
                    let connect_future = UnixStream::connect(&self.path);
                    UnixSinkState::Creating(Box::new(connect_future))
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
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(line)),
            Err(_) => {
                unreachable!(); // poll_ready() should never return an error
            }
            Ok(Async::Ready(connection)) => match connection.start_send(line) {
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
                    Ok(res)
                }
            },
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
    use crate::runtime::Runtime;
    use crate::test_util::{random_lines_with_stream, shutdown_on_idle};
    use futures01::{sync::mpsc, Sink, Stream};
    use stream_cancel::{StreamExt, Tripwire};
    use tokio01::codec::{FramedRead, LinesCodec};
    use tokio_uds::UnixListener;

    fn temp_uds_path(name: &str) -> PathBuf {
        tempfile::tempdir().unwrap().into_path().join(name)
    }

    #[test]
    fn unix_sink_healthcheck() {
        let path = temp_uds_path("valid_uds");
        let _listener = UnixListener::bind(&path).unwrap();
        let healthcheck = unix_healthcheck(path);
        assert!(healthcheck.wait().is_ok());

        let bad_path = temp_uds_path("no_one_listening");
        let bad_healthcheck = unix_healthcheck(bad_path);
        assert!(bad_healthcheck.wait().is_err());
    }

    #[test]
    fn basic_unix_sink() {
        let num_lines = 1000;
        let out_path = temp_uds_path("unix_test");

        // Set up Sink
        let config = UnixSinkConfig::new(out_path.clone(), Encoding::Text.into());
        let mut rt = Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());
        let (sink, _healthcheck) = config.build(cx).unwrap();

        // Set up server to receive events from the Sink.
        let listener = UnixListener::bind(&out_path).expect("failed to bind to listener socket");

        let (tx, rx) = mpsc::channel(num_lines);
        let (trigger, tripwire) = Tripwire::new();

        let receive_future = listener
            .incoming()
            .take_until(tripwire)
            .map_err(|e| error!("failed to accept socket; error = {:?}", e))
            .for_each(move |socket| {
                let tx = tx.clone();
                FramedRead::new(socket, LinesCodec::new())
                    .map_err(|e| error!("error reading line: {:?}", e))
                    .forward(tx.sink_map_err(|e| error!("error sending event: {:?}", e)))
                    .map(|_| ())
            });
        rt.spawn(receive_future);

        // Send the test data
        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).unwrap();
        drop(trigger);

        // Receive the data sent by the Sink to the receive_future
        let output_lines = rx.wait().map(Result::unwrap).collect::<Vec<_>>();
        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }
}
