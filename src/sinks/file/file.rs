use super::*;
use bytes::Bytes;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use futures::{try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend};
use tokio::codec::{BytesCodec, FramedWrite};
use tokio::fs::file::{File, OpenFuture};
use tokio::fs::OpenOptions;

use tracing::field;

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

pub struct FileSink {
    pub path: PathBuf,
    state: FileSinkState,
}

enum FileSinkState {
    Closed,
    OpeningFile(OpenFuture<PathBuf>),
    FileProvided(FramedWrite<File, BytesCodec>),
}

impl FileSinkState {
    fn init(path: PathBuf) -> Self {
        debug!(message = "opening", file = ?path);
        let mut options = OpenOptions::new();
        options.create(true).append(true);

        FileSinkState::OpeningFile(options.open(path))
    }
}

impl FileSink {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path: path.clone(),
            state: FileSinkState::init(path),
        }
    }

    pub fn new_with_encoding(path: &Path, encoding: Option<Encoding>) -> EmbeddedFileSink {
        let sink = FileSink::new(path.to_path_buf())
            .sink_map_err(|err| error!("Terminating the sink due to error: {}", err))
            .with(move |event| Self::encode_event(event, &encoding));

        Box::new(sink)
    }

    pub fn poll_file(&mut self) -> Poll<&mut FramedWrite<File, BytesCodec>, io::Error> {
        loop {
            match self.state {
                FileSinkState::Closed => return Err(closed()),

                FileSinkState::FileProvided(ref mut sink) => return Ok(Async::Ready(sink)),

                FileSinkState::OpeningFile(ref mut open_future) => match open_future.poll() {
                    Ok(Async::Ready(file)) => {
                        debug!(message = "provided", file = ?file);
                        self.state =
                            FileSinkState::FileProvided(FramedWrite::new(file, BytesCodec::new()));
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => {
                        self.state = FileSinkState::Closed;
                        return Err(err);
                    }
                },
            }
        }
    }

    fn encode_event(event: Event, encoding: &Option<Encoding>) -> Result<Bytes, ()> {
        let log = event.into_log();

        let result = match (encoding, log.is_structured()) {
            (&Some(Encoding::Json), _) | (_, true) => {
                serde_json::to_vec(&log.all_fields()).map_err(|e| panic!("Error encoding: {}", e))
            }

            (&Some(Encoding::Text), _) | (_, false) => Ok(log
                .get(&event::MESSAGE)
                .map(|v| v.as_bytes().to_vec())
                .unwrap_or(Vec::new())),
        };

        result.map(|mut bytes| {
            bytes.push(b'\n');
            Bytes::from(bytes)
        })
    }
}

impl Sink for FileSink {
    type SinkItem = Bytes;
    type SinkError = io::Error;

    fn start_send(&mut self, line: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match self.poll_file() {
            Ok(Async::Ready(file)) => {
                debug!(
                    message = "sending event",
                    bytes = &field::display(line.len())
                );
                match file.start_send(line) {
                    Ok(ok) => Ok(ok),

                    Err(err) => {
                        self.state = FileSinkState::Closed;
                        Err(err)
                    }
                }
            }
            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(line)),
            Err(err) => Err(err),
        }
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        if let FileSinkState::Closed = self.state {
            return Err(closed());
        }

        let file = try_ready!(self.poll_file());

        match file.poll_complete() {
            Err(err) => {
                error!("Error while completing {:?}: {}", self.path, err);
                self.state = FileSinkState::Closed;
                Ok(Async::Ready(()))
            }
            Ok(ok) => Ok(ok),
        }
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        match self.poll_complete() {
            Ok(Async::Ready(())) => match self.state {
                FileSinkState::Closed => Ok(Async::Ready(())),

                FileSinkState::FileProvided(ref mut sink) => sink.close(),

                //this state is eliminated during poll_complete()
                FileSinkState::OpeningFile(_) => unreachable!(),
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Err(err),
        }
    }
}

fn closed() -> io::Error {
    io::Error::new(ErrorKind::NotConnected, "FileSink is in closed state")
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        event::Event,
        test_util::{lines_from_file, random_lines_with_stream},
    };

    use futures::Stream;
    use tempfile::tempdir;

    #[test]
    fn text_output_is_correct() {
        let (input, events) = random_lines_with_stream(100, 16);
        let output = test_unpartitioned_with_encoding(events, Encoding::Text, None);

        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[test]
    fn json_output_is_correct() {
        let (input, events) = random_lines_with_stream(100, 16);
        let output = test_unpartitioned_with_encoding(events, Encoding::Json, None);

        for (input, output) in input.into_iter().zip(output) {
            let output: serde_json::Value = serde_json::from_str(&output[..]).unwrap();
            let output = output.get("message").and_then(|v| v.as_str()).unwrap();
            assert_eq!(input, output);
        }
    }

    #[test]
    fn file_is_appended_not_truncated() {
        let directory = tempdir().unwrap().into_path();

        let (mut input1, events) = random_lines_with_stream(100, 16);
        test_unpartitioned_with_encoding(events, Encoding::Text, Some(directory.clone()));

        let (mut input2, events) = random_lines_with_stream(100, 16);
        let output = test_unpartitioned_with_encoding(events, Encoding::Text, Some(directory));

        let mut input = vec![];
        input.append(&mut input1);
        input.append(&mut input2);

        assert_eq!(output.len(), input.len());

        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    fn test_unpartitioned_with_encoding<S>(
        events: S,
        encoding: Encoding,
        directory: Option<PathBuf>,
    ) -> Vec<String>
    where
        S: 'static + Stream<Item = Event, Error = ()> + Send,
    {
        let path = directory
            .unwrap_or(tempdir().unwrap().into_path())
            .join("test.out");

        let sink = FileSink::new_with_encoding(&path, Some(encoding));

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).unwrap();

        lines_from_file(&path)
    }

}
