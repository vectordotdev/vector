use super::Encoding;
use crate::event::{self, Event};
use bytes::{Bytes, BytesMut};
use codec::BytesDelimitedCodec;
use futures::{try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend};
use std::{ffi, io, path};
use tokio::{codec::Encoder, fs::file, io::AsyncWrite};

const INITIAL_CAPACITY: usize = 8 * 1024;
const BACKPRESSURE_BOUNDARY: usize = INITIAL_CAPACITY;

#[derive(Debug)]
pub struct File {
    state: State,
    encoding: Option<Encoding>,
    buffer: BytesMut,
    codec: BytesDelimitedCodec,
}

#[derive(Debug)]
enum State {
    Creating(file::OpenFuture<BytesPath>),
    Open(file::File),
    Closed,
}

impl File {
    pub fn new(path: Bytes, encoding: Option<Encoding>) -> Self {
        let path = BytesPath(path);

        let fut = file::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path);

        let state = State::Creating(fut);
        let buffer = BytesMut::with_capacity(INITIAL_CAPACITY);
        let codec = BytesDelimitedCodec::new(b'\n');

        File {
            state,
            encoding,
            buffer,
            codec,
        }
    }

    fn encode_event(&self, event: Event) -> Bytes {
        let log = event.into_log();

        match (&self.encoding, log.is_structured()) {
            (&Some(Encoding::Ndjson), _) | (None, true) => serde_json::to_vec(&log.unflatten())
                .map(Bytes::from)
                .expect("Unable to encode event as JSON."),
            (&Some(Encoding::Text), _) | (None, false) => log
                .get(&event::MESSAGE)
                .map(|v| v.as_bytes())
                .unwrap_or_default(),
        }
    }
}

// This implements a futures 0.3 based sink api that provides a `poll_ready`
// that doesn't require us to consume the event.
impl File {
    fn poll_ready(&mut self) -> Poll<(), io::Error> {
        match &mut self.state {
            State::Open(_file) => {
                // If the buffer is already over 8KiB, then attempt to flush it. If after flushing it's
                // *still* over 8KiB, then apply backpressure (reject the send).
                if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
                    self.poll_complete()?;

                    if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
                        return Ok(Async::NotReady);
                    }
                }
            }

            State::Creating(fut) => {
                let file = try_ready!(fut.poll());
                self.state = State::Open(file);
            }

            State::Closed => unreachable!(),
        }

        Ok(Async::Ready(()))
    }
}

impl Sink for File {
    type SinkItem = Event;
    type SinkError = io::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if let Async::NotReady = self.poll_ready()? {
            return Ok(AsyncSink::NotReady(item));
        }

        let event = self.encode_event(item);
        self.codec.encode(event, &mut self.buffer)?;
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), io::Error> {
        if let State::Open(file) = &mut self.state {
            trace!("flushing framed transport");

            while !self.buffer.is_empty() {
                trace!("writing; remaining={}", self.buffer.len());

                let n = try_ready!(file.poll_write(&self.buffer));

                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to \
                         write frame to transport",
                    ));
                }

                let _ = self.buffer.split_to(n);
            }

            // Try flushing the underlying IO
            try_ready!(file.poll_flush());

            trace!("framed transport flushed");
            Ok(Async::Ready(()))
        } else {
            unreachable!()
        }
    }

    fn close(&mut self) -> Poll<(), io::Error> {
        try_ready!(self.poll_complete());

        if let State::Open(file) = &mut self.state {
            try_ready!(file.shutdown());
        }

        self.state = State::Closed;
        Ok(Async::Ready(()))
    }
}

#[derive(Debug, Clone)]
struct BytesPath(Bytes);

impl AsRef<path::Path> for BytesPath {
    fn as_ref(&self) -> &path::Path {
        use std::os::unix::ffi::OsStrExt;
        let os_str = ffi::OsStr::from_bytes(&self.0[..]);
        &path::Path::new(os_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::Event,
        test_util::{lines_from_file, random_lines_with_stream, random_nested_events_with_stream},
    };
    use futures::Stream;
    use std::{collections::HashMap, path::PathBuf};
    use tempfile::tempdir;

    #[test]
    fn encode_text() {
        let (input, events) = random_lines_with_stream(100, 16);
        let output = test_unpartitioned_with_encoding(events, Encoding::Text, None);

        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[test]
    fn encode_json() {
        let (input, events) = random_nested_events_with_stream(4, 3, 3, 16);
        let output = test_unpartitioned_with_encoding(events, Encoding::Ndjson, None);

        for (input, output) in input.into_iter().zip(output) {
            let output: HashMap<String, HashMap<String, HashMap<String, String>>> =
                serde_json::from_str(&output[..]).unwrap();

            let deeper = input.into_log().unflatten().match_against(output).unwrap();
            for (input, output) in deeper {
                let deeper = input.match_against_map(output).unwrap();
                for (input, output) in deeper {
                    let deeper = input.match_against_map(output).unwrap();
                    for (input, output) in deeper {
                        assert!(input.equals(output))
                    }
                }
            }
        }
    }

    #[test]
    fn file_is_appended() {
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

        let b = Bytes::from(path.clone().to_str().unwrap().as_bytes());
        let sink = File::new(b, Some(encoding));

        let mut rt = crate::test_util::runtime();
        let pump = sink
            .sink_map_err(|e| panic!("error {:?}", e))
            .send_all(events);
        let _ = rt.block_on(pump).unwrap();

        lines_from_file(&path)
    }
}
