use std::time::{Duration, Instant};

use async_compression::tokio::write::GzipEncoder;
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use codecs::{
    encoding::{Framer, FramingConfig, SerializerConfig},
    JsonSerializerConfig, NewlineDelimitedEncoder, NewlineDelimitedEncoderConfig,
    TextSerializerConfig,
};
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
use tokio_util::codec::Encoder as _;
use vector_core::{buffers::Acker, internal_event::EventsSent, ByteSizeOf};

use crate::{
    codecs::Encoder,
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, SinkDescription,
    },
    event::{Event, EventStatus, Finalizable},
    expiring_hash_map::ExpiringHashMap,
    internal_events::{FileBytesSent, FileIoError, FileOpen, TemplateRenderingError},
    sinks::util::{
        encoding::{
            EncodingConfig, EncodingConfigWithFramingAdapter, EncodingConfigWithFramingMigrator,
            Transformer,
        },
        StreamSink,
    },
    template::Template,
};
mod bytes_path;
use std::convert::TryFrom;

use bytes_path::BytesPath;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingMigrator;

impl EncodingConfigWithFramingMigrator for EncodingMigrator {
    type Codec = Encoding;

    fn migrate(codec: &Self::Codec) -> (Option<FramingConfig>, SerializerConfig) {
        match codec {
            Encoding::Text => (None, TextSerializerConfig::new().into()),
            Encoding::Ndjson => (
                Some(NewlineDelimitedEncoderConfig::new().into()),
                JsonSerializerConfig::new().into(),
            ),
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileSinkConfig {
    pub path: Template,
    pub idle_timeout_secs: Option<u64>,
    #[serde(flatten)]
    pub encoding: EncodingConfigWithFramingAdapter<EncodingConfig<Encoding>, EncodingMigrator>,
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub compression: Compression,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

inventory::submit! {
    SinkDescription::new::<FileSinkConfig>("file")
}

impl GenerateConfig for FileSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            path: Template::try_from("/tmp/vector-%Y-%m-%d.log").unwrap(),
            idle_timeout_secs: None,
            encoding: EncodingConfig::from(Encoding::Text).into(),
            compression: Default::default(),
            acknowledgements: Default::default(),
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

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn sink_type(&self) -> &'static str {
        "file"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

#[derive(Debug)]
pub struct FileSink {
    acker: Acker,
    path: Template,
    transformer: Transformer,
    encoder: Encoder<Framer>,
    idle_timeout: Duration,
    files: ExpiringHashMap<Bytes, OutFile>,
    compression: Compression,
}

impl FileSink {
    pub fn new(config: &FileSinkConfig, acker: Acker) -> Self {
        let transformer = config.encoding.transformer();
        let (framer, serializer) = config.encoding.encoding();
        let framer = framer.unwrap_or_else(|| NewlineDelimitedEncoder::new().into());
        let encoder = Encoder::<Framer>::new(framer, serializer);

        Self {
            acker,
            path: config.path.clone(),
            transformer,
            encoder,
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
                emit!(TemplateRenderingError {
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
                                    emit!(FileIoError {
                                        error,
                                        code: "failed_closing_file",
                                        message: "Failed to close file.",
                                        path: Some(path),
                                    });
                                } else{
                                    trace!(message = "Successfully closed file.", path = ?path);
                                }
                            }

                            emit!(FileOpen {
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
                        Some((mut expired_file, path)) => {
                            // We got an expired file. All we really want is to
                            // flush and close it.
                            if let Err(error) = expired_file.close().await {
                                error!(message = "Failed to close file.", path = ?path, %error);
                            }
                            drop(expired_file); // ignore close error
                            emit!(FileOpen {
                                count: self.files.len()
                            });
                        }
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
                    emit!(FileIoError {
                        code: "failed_opening_file",
                        message: "Unable to open the file.",
                        error,
                        path: Some(&path),
                    });
                    event.metadata().update_status(EventStatus::Errored);
                    return;
                }
            };

            let outfile = OutFile::new(file, self.compression);

            self.files.insert_at(path.clone(), outfile, next_deadline);
            emit!(FileOpen {
                count: self.files.len()
            });
            self.files.get_mut(&path).unwrap()
        };

        trace!(message = "Writing an event to file.", path = ?path);
        let event_size = event.size_of();
        let finalizers = event.take_finalizers();
        match write_event_to_file(file, event, &self.transformer, &mut self.encoder).await {
            Ok(byte_size) => {
                finalizers.update_status(EventStatus::Delivered);
                emit!(EventsSent {
                    count: 1,
                    byte_size: event_size,
                    output: None,
                });
                emit!(FileBytesSent {
                    byte_size,
                    file: String::from_utf8_lossy(&path),
                });
            }
            Err(error) => {
                finalizers.update_status(EventStatus::Errored);
                emit!(FileIoError {
                    code: "failed_writing_file",
                    message: "Failed to write the file.",
                    error,
                    path: Some(&path),
                });
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

async fn write_event_to_file(
    file: &mut OutFile,
    mut event: Event,
    transformer: &Transformer,
    encoder: &mut Encoder<Framer>,
) -> Result<usize, std::io::Error> {
    transformer.transform(&mut event);
    let mut buffer = BytesMut::new();
    encoder
        .encode(event, &mut buffer)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    file.write_all(&buffer).await.map(|()| buffer.len())
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
    use vector_core::sink::VectorSink;

    use super::*;
    use crate::{
        config::log_schema,
        test_util::{
            components::{run_and_assert_sink_compliance, FILE_SINK_TAGS},
            lines_from_file, lines_from_gzip_file, random_events_with_stream,
            random_lines_with_stream, temp_dir, temp_file, trace_init,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<FileSinkConfig>();
    }

    #[tokio::test]
    async fn single_partition() {
        trace_init();

        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout_secs: None,
            encoding: EncodingConfig::from(Encoding::Text).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
        };

        let sink = FileSink::new(&config, Acker::passthrough());
        let (input, _events) = random_lines_with_stream(100, 64, None);

        let events = Box::pin(stream::iter(input.clone().into_iter().map(Event::from)));
        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(sink),
            events,
            &FILE_SINK_TAGS,
        )
        .await;

        let output = lines_from_file(template);
        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[tokio::test]
    async fn single_partition_gzip() {
        trace_init();

        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout_secs: None,
            encoding: EncodingConfig::from(Encoding::Text).into(),
            compression: Compression::Gzip,
            acknowledgements: Default::default(),
        };

        let sink = FileSink::new(&config, Acker::passthrough());
        let (input, _) = random_lines_with_stream(100, 64, None);

        let events = Box::pin(stream::iter(input.clone().into_iter().map(Event::from)));
        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(sink),
            events,
            &FILE_SINK_TAGS,
        )
        .await;

        let output = lines_from_gzip_file(template);
        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[tokio::test]
    async fn many_partitions() {
        trace_init();

        let directory = temp_dir();

        let mut template = directory.to_string_lossy().to_string();
        template.push_str("/{{level}}s-{{date}}.log");

        trace!(message = "Template.", %template);

        let config = FileSinkConfig {
            path: template.try_into().unwrap(),
            idle_timeout_secs: None,
            encoding: EncodingConfig::from(Encoding::Text).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
        };

        let sink = FileSink::new(&config, Acker::passthrough());

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
        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(sink),
            events,
            &FILE_SINK_TAGS,
        )
        .await;

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
        trace_init();

        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().try_into().unwrap(),
            idle_timeout_secs: Some(1),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            compression: Compression::None,
            acknowledgements: Default::default(),
        };

        let sink = FileSink::new(&config, Acker::passthrough());
        let (mut input, _events) = random_lines_with_stream(10, 64, None);

        let (mut tx, rx) = futures::channel::mpsc::channel(0);

        let sink_handle = tokio::spawn(async move {
            run_and_assert_sink_compliance(
                VectorSink::from_event_streamsink(sink),
                Box::pin(rx),
                &FILE_SINK_TAGS,
            )
            .await
        });

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

        // make sure sink stops and that it did not panic
        drop(tx);
        sink_handle.await.unwrap();
    }
}
