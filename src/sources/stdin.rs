use crate::{
    codecs::DecodingConfig,
    config::{log_schema, DataType, Resource, SourceConfig, SourceContext, SourceDescription},
    internal_events::{StdinEventReceived, StdinReadFailed},
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::{Bytes, BytesMut};
use futures::{channel::mpsc, executor, SinkExt, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use std::{io, thread};
use tokio_util::codec::Decoder;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct StdinConfig {
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    pub host_key: Option<String>,
    #[serde(flatten)]
    pub decoding: DecodingConfig,
}

impl Default for StdinConfig {
    fn default() -> Self {
        StdinConfig {
            max_length: default_max_length(),
            host_key: Default::default(),
            decoding: Default::default(),
        }
    }
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
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

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "stdin"
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::Stdin]
    }
}

pub fn stdin_source<R>(
    stdin: R,
    config: StdinConfig,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> crate::Result<super::Source>
where
    R: Send + io::BufRead + 'static,
{
    let host_key = config
        .host_key
        .unwrap_or_else(|| log_schema().host_key().to_string());
    let hostname = crate::get_hostname().ok();
    let mut decoder = config.decoding.build()?;

    let (mut sender, receiver) = mpsc::channel(1024);

    // Start the background thread
    thread::spawn(move || {
        info!("Capturing STDIN.");

        for line in stdin.lines() {
            if executor::block_on(sender.send(line)).is_err() {
                // receiver has closed so we should shutdown
                return;
            }
        }
    });

    Ok(Box::pin(async move {
        let mut out =
            out.sink_map_err(|error| error!(message = "Unable to send event to out.", %error));

        let mut lines = receiver
            .take_until(shutdown)
            .map_err(|error| emit!(StdinReadFailed { error }));

        while let Some(Ok(line)) = lines.next().await {
            emit!(StdinEventReceived {
                byte_size: line.len()
            });

            let mut bytes = BytesMut::from(line.as_bytes());

            loop {
                match decoder.decode_eof(&mut bytes) {
                    Ok(Some((events, _))) => {
                        for mut event in events {
                            let log = event.as_mut_log();

                            log.insert(log_schema().source_type_key(), Bytes::from("stdin"));

                            if let Some(hostname) = &hostname {
                                log.insert(&host_key, hostname.clone());
                            }

                            let _ = out.send(event).await;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => {
                        // Error is logged by `crate::codecs::Decoder`, no
                        // further handling is needed here.
                        break;
                    }
                }
            }
        }

        info!("Finished sending.");

        let _ = out.flush().await; // error emitted by sink_map_err

        Ok(())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_util::trace_init, Pipeline};
    use std::io::Cursor;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StdinConfig>();
    }

    #[tokio::test]
    async fn stdin_decodes_line() {
        trace_init();

        let (tx, rx) = Pipeline::new_test();
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
    }
}
