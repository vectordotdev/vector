use crate::{
    buffers::Acker,
    config::SinkContext,
    internal_events::{
        UnixSocketConnectionEstablished, UnixSocketConnectionFailure, UnixSocketError,
        UnixSocketEventSent,
    },
    sinks::{
        util::{encode_event, encoding::EncodingConfig, Encoding, StreamSink},
        Healthcheck, VectorSink,
    },
    Event,
};
use async_trait::async_trait;
use futures::{stream::BoxStream, FutureExt, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{path::PathBuf, time::Duration};
use tokio::{net::UnixStream, time::delay_for};
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

    pub fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let encoding = self.encoding.clone();
        let sink = UnixSink::new(self.path.clone(), cx.acker(), encoding);
        let healthcheck = healthcheck(self.path.clone()).boxed();
        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
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

    async fn next_delay(&mut self) {
        delay_for(self.backoff.next().unwrap()).await
    }

    async fn get_stream(&mut self) -> &mut FramedWrite<UnixStream, BytesCodec> {
        loop {
            match self.state {
                UnixSinkState::Connected(ref mut stream) => return stream,
                UnixSinkState::Disconnected => {
                    debug!(
                        message = "Connecting",
                        path = &field::display(self.path.to_str().unwrap())
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
            let bytes = match encode_event(event, &self.encoding) {
                Some(bytes) => bytes,
                None => continue,
            };

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
        sink.run(events).await.unwrap();

        // Wait for output to connect
        receiver.connected().await;

        // Receive the data sent by the Sink to the receiver
        assert_eq!(input_lines, receiver.await);
    }
}
