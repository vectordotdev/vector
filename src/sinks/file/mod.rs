use crate::expiring_hash_map::ExpiringHashMap;
use crate::{
    event::{self, Event},
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        StreamSink,
    },
    template::Template,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::pin_mut;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};

mod bytes_path;
use bytes_path::BytesPath;

use super::streaming_sink::{self, StreamingSink};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileSinkConfig {
    pub path: Template,
    pub idle_timeout_secs: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub encoding: EncodingConfigWithDefault<Encoding>,
}

inventory::submit! {
    SinkDescription::new_without_default::<FileSinkConfig>("file")
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Ndjson,
}

impl Default for Encoding {
    fn default() -> Self {
        Encoding::Text
    }
}

#[typetag::serde(name = "file")]
impl SinkConfig for FileSinkConfig {
    fn build(&self, mut cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = FileSink::new(&self);
        let sink = streaming_sink::compat::adapt_to_topology(&mut cx, sink);
        let sink = StreamSink::new(sink, cx.acker());
        Ok((Box::new(sink), Box::new(futures01::future::ok(()))))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "file"
    }
}

#[derive(Debug)]
pub struct FileSink {
    path: Template,
    encoding: EncodingConfigWithDefault<Encoding>,
    idle_timeout: Duration,
    files: ExpiringHashMap<Bytes, File>,
}

impl FileSink {
    pub fn new(config: &FileSinkConfig) -> Self {
        Self {
            path: config.path.clone(),
            encoding: config.encoding.clone(),
            idle_timeout: Duration::from_secs(config.idle_timeout_secs.unwrap_or(30)),
            files: ExpiringHashMap::new(),
        }
    }

    /// Uses pass the `event` to `self.path` template to obtain the file path
    /// to store the event as.
    fn partition_event(&mut self, event: &Event) -> Option<bytes::Bytes> {
        let bytes = match self.path.render(event) {
            Ok(b) => b,
            Err(missing_keys) => {
                warn!(
                    message = "Keys do not exist on the event. Dropping event.",
                    ?missing_keys
                );
                return None;
            }
        };

        Some(bytes)
    }

    fn deadline_at(&self) -> Instant {
        Instant::now()
            .checked_add(self.idle_timeout)
            .expect("unable to compute next deadline")
    }

    async fn run(&mut self, input: impl Stream<Item = Event> + Send + Sync) -> crate::Result<()> {
        pin_mut!(input);
        loop {
            tokio::select! {
                event = input.next() => {
                    match event {
                        None => {
                            // If we got `None` - terminate the processing.
                            debug!(message = "Receiver exausted, terminating the processing loop.");
                            break;
                        }
                        Some(event) => self.process_event(event).await,
                    }
                }
                result = self.files.next_expired(), if !self.files.is_empty() => {
                    match result {
                        // We do not poll map when it's empty, so we should
                        // never reach this branch.
                        None => unreachable!(),
                        Some(Ok((mut expired_file, path))) => {
                            // We got an expired file. All we really want is to
                            // flush and close it.
                            if let Err(error) = expired_file.flush().await {
                                error!(message = "Failed to flush file.", ?path, %error);
                            }
                            drop(expired_file); // ignore close error
                        }
                        Some(Err(error)) => error!(
                            message = "An error occured while expiring a file.",
                            %error,
                        ),
                    }
                }
            }
        }
        Ok(())
    }

    async fn process_event(&mut self, event: Event) {
        let path = match self.partition_event(&event) {
            Some(path) => path,
            None => {
                // We weren't able to find the path to use for the
                // file.
                // This is already logged at `partition_event`, so
                // here we just skip the event.
                return;
            }
        };

        let next_deadline = self.deadline_at();
        trace!(message = "Computed next deadline.", ?next_deadline, ?path);

        let file = if let Some(file) = self.files.reset_at(&path, next_deadline) {
            trace!(message = "Working with an already opened file.", ?path);
            file
        } else {
            trace!(message = "Opening new file.", ?path);
            let file = match open_file(BytesPath::new(path.clone())).await {
                Ok(file) => file,
                Err(error) => {
                    // We coundn't open the file for this event.
                    // Maybe other events will work though! Just log
                    // the error and skip this event.
                    error!(message = "Unable to open the file.", ?path, %error);
                    return;
                }
            };
            self.files.insert_at(path.clone(), file, next_deadline);
            self.files.get_mut(&path).unwrap()
        };

        trace!(message = "Writing an event to file.", ?path);
        if let Err(error) = write_event_to_file(file, event, &self.encoding).await {
            error!(message = "Failed to write file.", ?path, %error);
        }
    }
}

async fn open_file(path: impl AsRef<std::path::Path>) -> std::io::Result<File> {
    let parent = path.as_ref().parent();

    if let Some(parent) = parent {
        fs::create_dir_all(parent).await?;
    }

    fs::OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .open(path)
        .await
}

pub fn encode_event(encoding: &EncodingConfigWithDefault<Encoding>, mut event: Event) -> Vec<u8> {
    encoding.apply_rules(&mut event);
    let log = event.into_log();
    match encoding.codec() {
        Encoding::Ndjson => serde_json::to_vec(&log).expect("Unable to encode event as JSON."),
        Encoding::Text => log
            .get(&event::log_schema().message_key())
            .map(|v| v.to_string_lossy().into_bytes())
            .unwrap_or_default(),
    }
}

async fn write_event_to_file(
    file: &mut File,
    event: Event,
    encoding: &EncodingConfigWithDefault<Encoding>,
) -> Result<(), std::io::Error> {
    let mut buf = encode_event(encoding, event);
    buf.push(b'\n');
    file.write_all(&buf[..]).await
}

#[async_trait]
impl StreamingSink for FileSink {
    async fn run(
        &mut self,
        input: impl Stream<Item = Event> + Send + Sync + 'static,
    ) -> crate::Result<()> {
        FileSink::run(self, input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event,
        test_util::{
            self, lines_from_file, random_events_with_stream, random_lines_with_stream, temp_dir,
            temp_file,
        },
    };
    use futures::stream;
    use std::convert::TryInto;

    #[test]
    fn single_partition() {
        test_util::trace_init();

        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout_secs: None,
            encoding: Encoding::Text.into(),
        };

        let mut sink = FileSink::new(&config);
        let (input, _) = random_lines_with_stream(100, 64);

        let events = stream::iter(input.clone().into_iter().map(Event::from));

        let mut rt = crate::test_util::runtime();
        let _ = rt
            .block_on_std(async move { sink.run(events).await })
            .unwrap();

        let output = lines_from_file(template);
        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[test]
    fn many_partitions() {
        test_util::trace_init();

        let directory = temp_dir();

        let mut template = directory.to_string_lossy().to_string();
        template.push_str("/{{level}}s-{{date}}.log");

        trace!(message = "Template", %template);

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout_secs: None,
            encoding: Encoding::Text.into(),
        };

        let mut sink = FileSink::new(&config);

        let (mut input, _) = random_events_with_stream(32, 8);
        input[0].as_mut_log().insert("date", "2019-26-07");
        input[0].as_mut_log().insert("level", "warning");
        input[1].as_mut_log().insert("date", "2019-26-07");
        input[1].as_mut_log().insert("level", "error");
        input[2].as_mut_log().insert("date", "2019-26-07");
        input[2].as_mut_log().insert("level", "warning");
        input[3].as_mut_log().insert("date", "2019-27-07");
        input[3].as_mut_log().insert("level", "error");
        input[4].as_mut_log().insert("date", "2019-27-07");
        input[4].as_mut_log().insert("level", "warning");
        input[5].as_mut_log().insert("date", "2019-27-07");
        input[5].as_mut_log().insert("level", "warning");
        input[6].as_mut_log().insert("date", "2019-28-07");
        input[6].as_mut_log().insert("level", "warning");
        input[7].as_mut_log().insert("date", "2019-29-07");
        input[7].as_mut_log().insert("level", "error");

        let events = stream::iter(input.clone().into_iter());
        let mut rt = crate::test_util::runtime();
        let _ = rt
            .block_on_std(async move { sink.run(events).await })
            .unwrap();

        let output = vec![
            lines_from_file(&directory.join("warnings-2019-26-07.log")),
            lines_from_file(&directory.join("errors-2019-26-07.log")),
            lines_from_file(&directory.join("warnings-2019-27-07.log")),
            lines_from_file(&directory.join("errors-2019-27-07.log")),
            lines_from_file(&directory.join("warnings-2019-28-07.log")),
            lines_from_file(&directory.join("errors-2019-29-07.log")),
        ];

        assert_eq!(
            input[0].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[0][0])
        );
        assert_eq!(
            input[1].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[1][0])
        );
        assert_eq!(
            input[2].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[0][1])
        );
        assert_eq!(
            input[3].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[3][0])
        );
        assert_eq!(
            input[4].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[2][0])
        );
        assert_eq!(
            input[5].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[2][1])
        );
        assert_eq!(
            input[6].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[4][0])
        );
        assert_eq!(
            input[7].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[5][0])
        );
    }
}
