use crate::{
    template::Template,
    topology::config::{DataType, SinkConfig, SinkContext},
    Event, Result,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{Poll, Sink, StartSend};
use futures03::channel::mpsc::{channel, Receiver, Sender};
use futures03::compat::CompatSink;
use futures03::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{ffi, path};
use tokio02::{fs::File, io::AsyncWriteExt};

// === StreamingSink ===

#[async_trait]
pub trait StreamingSink: Send + Sync + 'static {
    async fn run(&mut self, input: Receiver<Event>) -> Result<()>;

    fn build_sink(self) -> super::RouterSink
    where
        Self: Sized + 'static,
    {
        let (tx, rx) = channel(64);

        let sink = LazyStreamingSink {
            sink: Some((rx, self)),
            inner: CompatSink::new(tx),
        };

        Box::new(sink)
    }
}

pub struct LazyStreamingSink<T> {
    sink: Option<(Receiver<Event>, T)>,
    inner: CompatSink<Sender<Event>, Event>,
}

impl<T: StreamingSink> Sink for LazyStreamingSink<T> {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if let Some((rx, mut sink)) = self.sink.take() {
            tokio02::spawn(async move {
                if let Err(error) = sink.run(rx).await {
                    error!(message = "Unexpected sink failure.", %error);
                }
            });
        }

        self.inner.start_send(item).map_err(drop)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.poll_complete().map_err(drop)
    }
}

// === File Sink Implementation ===

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileSinkConfig {
    pub path: Template,
    pub idle_timeout_secs: Option<u64>,
    pub encoding: Encoding,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Ndjson,
}

#[typetag::serde(name = "file2")]
impl SinkConfig for FileSinkConfig {
    fn build(&self, _cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = FileSink {
            path: self.path.clone(),
            files: HashMap::new(),
            encoding: self.encoding.clone(),
        }
        .build_sink();

        Ok((sink, Box::new(futures::future::ok(()))))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "file2"
    }
}

struct FileSink {
    path: Template,
    files: HashMap<Bytes, File>,
    encoding: Encoding,
}

impl FileSink {
    fn partition_event(&mut self, event: &Event) -> Option<Bytes> {
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
}

#[async_trait]
impl StreamingSink for FileSink {
    async fn run(&mut self, mut input: Receiver<Event>) -> Result<()> {
        while let Some(event) = input.next().await {
            if let Some(path) = self.partition_event(&event) {
                let mut file = if let Some(file) = self.files.get_mut(&path) {
                    file
                } else {
                    let file = File::create(BytesPath::new(path.clone())).await.unwrap();
                    self.files.insert(path.clone(), file);

                    self.files.get_mut(&path).unwrap()
                };

                let log = event.into_log();

                let mut buf = match self.encoding {
                    Encoding::Ndjson => serde_json::to_vec(&log.unflatten())
                        .expect("Unable to encode event as JSON."),
                    Encoding::Text => log
                        .get(&crate::event::log_schema().message_key())
                        .map(|v| v.to_string_lossy().into_bytes())
                        .unwrap_or_default(),
                };

                buf.push(b'\n');

                let encoded_event: Vec<u8> = buf.into();

                file.write_all(&encoded_event[..]).await.unwrap();
            }
        }

        Ok(())
    }
}

// === Fun little hack around bytse and OsStr ===

#[derive(Debug, Clone)]
struct BytesPath {
    #[cfg(unix)]
    path: Bytes,
    #[cfg(windows)]
    path: path::PathBuf,
}

impl BytesPath {
    #[cfg(unix)]
    fn new(path: Bytes) -> BytesPath {
        BytesPath { path }
    }
    #[cfg(windows)]
    fn new(path: Bytes) -> BytesPath {
        let utf8_string = String::from_utf8_lossy(&path[..]);
        let path = path::PathBuf::from(utf8_string.as_ref());
        BytesPath { path }
    }
}

impl AsRef<path::Path> for BytesPath {
    #[cfg(unix)]
    fn as_ref(&self) -> &path::Path {
        use std::os::unix::ffi::OsStrExt;
        let os_str = ffi::OsStr::from_bytes(&self.path);
        &path::Path::new(os_str)
    }
    #[cfg(windows)]
    fn as_ref(&self) -> &path::Path {
        &self.path.as_ref()
    }
}
