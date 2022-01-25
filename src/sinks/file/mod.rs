use std::time::{Duration, Instant};

use async_compression::tokio::write::GzipEncoder;
use async_trait::async_trait;
use bytes::Bytes;
use futures::{
    future,
    stream::{BoxStream, StreamExt},
    FutureExt,
};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};
use vector_core::{buffers::Acker, internal_event::EventsSent, ByteSizeOf};

use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::{Event, EventStatus, Finalizable},
    expiring_hash_map::ExpiringHashMap,
    internal_events::{FileBytesSent, FileOpen, TemplateRenderingFailed},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        StreamSink,
    },
    template::Template,
};
mod bytes_path;
use std::convert::TryFrom;

use bytes_path::BytesPath;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileSinkConfig {
    pub path: Template,
    pub idle_timeout_secs: Option<u64>,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub compression: Compression,
}

inventory::submit! {
    SinkDescription::new::<FileSinkConfig>("file")
}

impl GenerateConfig for FileSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            path: Template::try_from("/tmp/vector-%Y-%m-%d.log").unwrap(),
            idle_timeout_secs: None,
            encoding: Encoding::Text.into(),
            compression: Default::default(),
        })
        .unwrap()
    }
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Ndjson,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    Gzip,
    None,
}

impl Default for Compression {
    fn default() -> Self {
        Compression::None
    }
}

enum OutFile {
    Regular(File),
    Gzip(GzipEncoder<File>),
}

impl OutFile {
    fn new(file: File, compression: Compression) -> Self {
        match compression {
            Compression::None => OutFile::Regular(file),
            Compression::Gzip => OutFile::Gzip(GzipEncoder::new(file)),
        }
    }

    async fn sync_all(&mut self) -> Result<(), std::io::Error> {
        match self {
            OutFile::Regular(file) => file.sync_all().await,
            OutFile::Gzip(gzip) => gzip.get_mut().sync_all().await,
        }
    }

    async fn shutdown(&mut self) -> Result<(), std::io::Error> {
        match self {
            OutFile::Regular(file) => file.shutdown().await,
            OutFile::Gzip(gzip) => gzip.shutdown().await,
        }
    }

    async fn write_all(&mut self, src: &[u8]) -> Result<(), std::io::Error> {
        match self {
            OutFile::Regular(file) => file.write_all(src).await,
            OutFile::Gzip(gzip) => gzip.write_all(src).await,
        }
    }

    /// Shutdowns by flushing data, writing headers, and syncing all of that
    /// data and metadata to the filesystem.
    async fn close(&mut self) -> Result<(), std::io::Error> {
        self.shutdown().await?;
        self.sync_all().await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "file")]
impl SinkConfig for FileSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = FileSink::new(self, cx.acker());
        Ok((
            super::VectorSink::from_event_streamsink(sink),
            future::ok(()).boxed(),
        ))
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
    acker: Acker,
    path: Template,
    encoding: EncodingConfig<Encoding>,
    idle_timeout: Duration,
    files: ExpiringHashMap<Bytes, OutFile>,
    compression: Compression,
}

impl FileSink {
    pub fn new(config: &FileSinkConfig, acker: Acker) -> Self {
        Self {
            acker,
            path: config.path.clone(),
            encoding: config.encoding.clone(),
            idle_timeout: Duration::from_secs(config.idle_timeout_secs.unwrap_or(30)),
            files: ExpiringHashMap::default(),
            compression: config.compression,
        }
    }

    /// Uses pass the `event` to `self.path` template to obtain the file path
    /// to store the event as.
    fn partition_event(&mut self, event: &Event) -> Option<bytes::Bytes> {
        let bytes = match self.path.render(event) {
            Ok(b) => b,
            Err(error) => {
                emit!(&TemplateRenderingFailed {
                    error,
                    field: Some("path"),
                    drop_event: true,
                });
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

    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> crate::Result<()> {
        loop {
            tokio::select! {
                event = input.next() => {
                    match event {
                        Some(event) => {
                            self.process_event(event).await;
                            self.acker.ack(1);
                        },
                        None => {
                            // If we got `None` - terminate the processing.
                            debug!(message = "Receiver exhausted, terminating the processing loop.");

                            // Close all the open files.
                            debug!(message = "Closing all the open files.");
                            for (path, file) in self.files.iter_mut() {
                                if let Err(error) = file.close().await {
                                    error!(message = "Failed to close file.", path = ?path, %error);
                                } else{
                                    trace!(message = "Successfully closed file.", path = ?path);
                                }
                            }

                            emit!(&FileOpen {
                                count: 0
                            });

                            break;
                        }
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
                            if let Err(error) = expired_file.close().await {
                                error!(message = "Failed to close file.", path = ?path, %error);
                            }
                            drop(expired_file); // ignore close error
                            emit!(&FileOpen {
                                count: self.files.len()
                            });
                        }
                        Some(Err(error)) => error!(
                            message = "An error occurred while expiring a file.",
                            %error,
                        ),
                    }
                }
            }
        }

        Ok(())
    }

    async fn process_event(&mut self, mut event: Event) {
        let path = match self.partition_event(&event) {
            Some(path) => path,
            None => {
                // We weren't able to find the path to use for the
                // file.
                // This is already logged at `partition_event`, so
                // here we just skip the event.
                event.metadata().update_status(EventStatus::Errored);
                return;
            }
        };

        let next_deadline = self.deadline_at();
        trace!(message = "Computed next deadline.", next_deadline = ?next_deadline, path = ?path);

        let file = if let Some(file) = self.files.reset_at(&path, next_deadline) {
            trace!(message = "Working with an already opened file.", path = ?path);
            file
        } else {
            trace!(message = "Opening new file.", ?path);
            let file = match open_file(BytesPath::new(path.clone())).await {
                Ok(file) => file,
                Err(error) => {
                    // We couldn't open the file for this event.
                    // Maybe other events will work though! Just log
                    // the error and skip this event.
                    error!(message = "Unable to open the file.", path = ?path, %error);
                    event.metadata().update_status(EventStatus::Errored);
                    return;
                }
            };

            let outfile = OutFile::new(file, self.compression);

            self.files.insert_at(path.clone(), outfile, next_deadline);
            emit!(&FileOpen {
                count: self.files.len()
            });
            self.files.get_mut(&path).unwrap()
        };

        trace!(message = "Writing an event to file.", path = ?path);
        let event_size = event.size_of();
        let finalizers = event.take_finalizers();
        match write_event_to_file(file, event, &self.encoding).await {
            Ok(byte_size) => {
                finalizers.update_status(EventStatus::Delivered);
                emit!(&EventsSent {
                    count: 1,
                    byte_size: event_size,
                    output: None,
                });
                emit!(&FileBytesSent {
                    byte_size,
                    file: String::from_utf8_lossy(&path),
                });
            }
            Err(error) => {
                finalizers.update_status(EventStatus::Errored);
                error!(message = "Failed to write file.", path = ?path, %error);
            }
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
        .append(true)
        .open(path)
        .await
}

pub fn encode_event(encoding: &EncodingConfig<Encoding>, mut event: Event) -> Vec<u8> {
    encoding.apply_rules(&mut event);
    let log = event.into_log();
    match encoding.codec() {
        Encoding::Ndjson => serde_json::to_vec(&log).expect("Unable to encode event as JSON."),
        Encoding::Text => log
            .get(log_schema().message_key())
            .map(|v| v.to_string_lossy().into_bytes())
            .unwrap_or_default(),
    }
}

async fn write_event_to_file(
    file: &mut OutFile,
    event: Event,
    encoding: &EncodingConfig<Encoding>,
) -> Result<usize, std::io::Error> {
    let mut buf = encode_event(encoding, event);
    buf.push(b'\n');
    file.write_all(&buf[..]).await.map(|()| buf.len())
}

#[async_trait]
impl StreamSink<Event> for FileSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        FileSink::run(&mut self, input)
            .await
            .expect("file sink error");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use futures::{stream, SinkExt};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_util::{
        components::{self, FILE_SINK_TAGS, SINK_TESTS},
        lines_from_file, lines_from_gzip_file, random_events_with_stream, random_lines_with_stream,
        temp_dir, temp_file, trace_init,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<FileSinkConfig>();
    }

    #[tokio::test]
    async fn single_partition() {
        components::init_test();
        trace_init();

        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout_secs: None,
            encoding: Encoding::Text.into(),
            compression: Compression::None,
        };

        let mut sink = FileSink::new(&config, Acker::passthrough());
        let (input, _events) = random_lines_with_stream(100, 64, None);

        let events = Box::pin(stream::iter(input.clone().into_iter().map(Event::from)));
        sink.run(events).await.unwrap();
        SINK_TESTS.assert(&FILE_SINK_TAGS);

        let output = lines_from_file(template);
        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[tokio::test]
    async fn single_partition_gzip() {
        components::init_test();
        trace_init();

        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout_secs: None,
            encoding: Encoding::Text.into(),
            compression: Compression::Gzip,
        };

        let mut sink = FileSink::new(&config, Acker::passthrough());
        let (input, _) = random_lines_with_stream(100, 64, None);

        let events = Box::pin(stream::iter(input.clone().into_iter().map(Event::from)));
        sink.run(events).await.unwrap();
        SINK_TESTS.assert(&FILE_SINK_TAGS);

        let output = lines_from_gzip_file(template);
        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[tokio::test]
    async fn many_partitions() {
        components::init_test();
        trace_init();

        let directory = temp_dir();

        let mut template = directory.to_string_lossy().to_string();
        template.push_str("/{{level}}s-{{date}}.log");

        trace!(message = "Template.", %template);

        let config = FileSinkConfig {
            path: template.try_into().unwrap(),
            idle_timeout_secs: None,
            encoding: Encoding::Text.into(),
            compression: Compression::None,
        };

        let mut sink = FileSink::new(&config, Acker::passthrough());

        let (mut input, _events) = random_events_with_stream(32, 8, None);
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

        let events = Box::pin(stream::iter(input.clone().into_iter()));
        sink.run(events).await.unwrap();
        SINK_TESTS.assert(&FILE_SINK_TAGS);

        let output = vec![
            lines_from_file(&directory.join("warnings-2019-26-07.log")),
            lines_from_file(&directory.join("errors-2019-26-07.log")),
            lines_from_file(&directory.join("warnings-2019-27-07.log")),
            lines_from_file(&directory.join("errors-2019-27-07.log")),
            lines_from_file(&directory.join("warnings-2019-28-07.log")),
            lines_from_file(&directory.join("errors-2019-29-07.log")),
        ];

        assert_eq!(
            input[0].as_log()[log_schema().message_key()],
            From::<&str>::from(&output[0][0])
        );
        assert_eq!(
            input[1].as_log()[log_schema().message_key()],
            From::<&str>::from(&output[1][0])
        );
        assert_eq!(
            input[2].as_log()[log_schema().message_key()],
            From::<&str>::from(&output[0][1])
        );
        assert_eq!(
            input[3].as_log()[log_schema().message_key()],
            From::<&str>::from(&output[3][0])
        );
        assert_eq!(
            input[4].as_log()[log_schema().message_key()],
            From::<&str>::from(&output[2][0])
        );
        assert_eq!(
            input[5].as_log()[log_schema().message_key()],
            From::<&str>::from(&output[2][1])
        );
        assert_eq!(
            input[6].as_log()[log_schema().message_key()],
            From::<&str>::from(&output[4][0])
        );
        assert_eq!(
            input[7].as_log()[log_schema().message_key()],
            From::<&str>::from(&output[5][0])
        );
    }

    #[tokio::test]
    async fn reopening() {
        components::init_test();
        trace_init();

        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout_secs: Some(1),
            encoding: Encoding::Text.into(),
            compression: Compression::None,
        };

        let mut sink = FileSink::new(&config, Acker::passthrough());
        let (mut input, _events) = random_lines_with_stream(10, 64, None);

        let (mut tx, rx) = futures::channel::mpsc::channel(0);

        let _ = tokio::spawn(async move { sink.run(Box::pin(rx)).await });

        // send initial payload
        for line in input.clone() {
            tx.send(Event::from(line)).await.unwrap();
        }

        // wait for file to go idle and be closed
        tokio::time::sleep(Duration::from_secs(2)).await;

        // trigger another write
        let last_line = "i should go at the end";
        tx.send(Event::from(last_line)).await.unwrap();
        input.push(String::from(last_line));

        // wait for another flush
        tokio::time::sleep(Duration::from_secs(1)).await;

        // make sure we appended instead of overwriting
        let output = lines_from_file(template);
        assert_eq!(input, output);

        SINK_TESTS.assert(&FILE_SINK_TAGS);
    }
}
