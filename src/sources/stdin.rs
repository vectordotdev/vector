use std::{io, thread};

use async_stream::stream;
use bytes::Bytes;
use chrono::Utc;
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use futures::{channel::mpsc, executor, SinkExt, StreamExt};
use tokio_util::{codec::FramedRead, io::StreamReader};
use vector_config::configurable_component;
use vector_core::ByteSizeOf;

use crate::{
    codecs::DecodingConfig,
    config::{log_schema, Output, Resource, SourceConfig, SourceContext, SourceDescription},
    internal_events::{BytesReceived, OldEventsReceived, StreamClosedError},
    serde::default_decoding,
    shutdown::ShutdownSignal,
    SourceSender,
};
/// Configuration for the `stdin` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct StdinConfig {
    /// The maximum buffer size, in bytes, of incoming messages.
    ///
    /// Messages larger than this are truncated.
    #[serde(default = "crate::serde::default_max_length")]
    pub max_length: usize,

    /// Overrides the name of the log field used to add the current hostname to each event.
    ///
    /// The value will be the current hostname for wherever Vector is running.
    ///
    /// By default, the [global `host_key` option](https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key) is used.
    pub host_key: Option<String>,

    #[configurable(derived)]
    pub framing: Option<FramingConfig>,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,
}

impl Default for StdinConfig {
    fn default() -> Self {
        StdinConfig {
            max_length: crate::serde::default_max_length(),
            host_key: Default::default(),
            framing: None,
            decoding: default_decoding(),
        }
    }
}

inventory::submit! {
    SourceDescription::new::<StdinConfig>("stdin")
}

impl_generate_config_from_default!(StdinConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "stdin")]
impl SourceConfig for StdinConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        stdin_source(
            io::BufReader::new(io::stdin()),
            self.clone(),
            cx.shutdown,
            cx.out,
        )
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
    }

    fn source_type(&self) -> &'static str {
        "stdin"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::Stdin]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

pub fn stdin_source<R>(
    mut stdin: R,
    config: StdinConfig,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> crate::Result<super::Source>
where
    R: Send + io::BufRead + 'static,
{
    let host_key = config
        .host_key
        .unwrap_or_else(|| log_schema().host_key().to_string());
    let hostname = crate::get_hostname().ok();

    let framing = config
        .framing
        .unwrap_or_else(|| config.decoding.default_stream_framing());
    let decoder = DecodingConfig::new(framing, config.decoding).build();

    let (mut sender, receiver) = mpsc::channel(1024);

    // Spawn background thread with blocking I/O to process stdin.
    //
    // This is recommended by Tokio, as otherwise the process will not shut down
    // until another newline is entered. See
    // https://github.com/tokio-rs/tokio/blob/a73428252b08bf1436f12e76287acbc4600ca0e5/tokio/src/io/stdin.rs#L33-L42
    thread::spawn(move || {
        info!("Capturing STDIN.");

        loop {
            let (buffer, len) = match stdin.fill_buf() {
                Ok(buffer) if buffer.is_empty() => break, // EOF.
                Ok(buffer) => (Ok(Bytes::copy_from_slice(buffer)), buffer.len()),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => (Err(error), 0),
            };

            stdin.consume(len);

            if executor::block_on(sender.send(buffer)).is_err() {
                // Receiver has closed so we should shutdown.
                break;
            }
        }
    });

    Ok(Box::pin(async move {
        let stream = StreamReader::new(receiver);
        let mut stream = FramedRead::new(stream, decoder).take_until(shutdown);
        let mut stream = stream! {
            while let Some(result) = stream.next().await {
                match result {
                    Ok((events, byte_size)) => {
                        emit!(BytesReceived { byte_size, protocol: "none" });

                        emit!(OldEventsReceived {
                            byte_size: events.size_of(),
                            count: events.len()
                        });

                        let now = Utc::now();

                        for mut event in events {
                            let log = event.as_mut_log();

                            log.try_insert(log_schema().source_type_key(), Bytes::from("stdin"));
                            log.try_insert(log_schema().timestamp_key(), now);

                            if let Some(hostname) = &hostname {
                                log.try_insert(host_key.as_str(), hostname.clone());
                            }

                            yield event;
                        }
                    }
                    Err(error) => {
                        // Error is logged by `crate::codecs::Decoder`, no
                        // further handling is needed here.
                        if !error.can_continue() {
                            break;
                        }
                    }
                }
            }
        }
        .boxed();

        match out.send_event_stream(&mut stream).await {
            Ok(()) => {
                info!("Finished sending.");
                Ok(())
            }
            Err(error) => {
                let (count, _) = stream.size_hint();
                emit!(StreamClosedError { error, count });
                Err(())
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::{test_util::components::assert_source_compliance, SourceSender};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StdinConfig>();
    }

    #[tokio::test]
    async fn stdin_decodes_line() {
        assert_source_compliance(&["protocol"], async {
            let (tx, rx) = SourceSender::new_test();
            let config = StdinConfig::default();
            let buf = Cursor::new("hello world\nhello world again");

            stdin_source(buf, config, ShutdownSignal::noop(), tx)
                .unwrap()
                .await
                .unwrap();

            let mut stream = rx;

            let event = stream.next().await;
            assert_eq!(
                Some("hello world".into()),
                event.map(|event| event.as_log()[log_schema().message_key()].to_string_lossy())
            );

            let event = stream.next().await;
            assert_eq!(
                Some("hello world again".into()),
                event.map(|event| event.as_log()[log_schema().message_key()].to_string_lossy())
            );

            let event = stream.next().await;
            assert!(event.is_none());
        })
        .await;
    }
}
