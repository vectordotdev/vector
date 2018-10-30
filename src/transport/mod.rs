pub mod file;

pub use self::file::*;

use futures::{Async, AsyncSink, Future, Poll, Sink, StartSend, Stream};
use std::io::{self, BufReader, BufWriter, Read, SeekFrom};
use std::path::{Path, PathBuf};
use tokio::{
    codec::{FramedRead, FramedWrite, LinesCodec},
    fs::{file::CreateFuture, read_dir, File},
    io::AsyncRead,
};

pub fn read_log(data_dir: &str) -> impl Stream<Item = String, Error = io::Error> {
    let data_dir = PathBuf::from(data_dir);
    read_dir(data_dir.clone())
        .flatten_stream()
        .collect()
        .and_then(|entries| {
            let biggest = entries
                .into_iter()
                .map(|e| e.path())
                .filter(|p| p.extension().map(|e| e == "log").unwrap_or(false))
                .max()
                .unwrap();
            info!("opening {:?} for reading", biggest);
            File::open(biggest)
        }).and_then(|file| file.seek(SeekFrom::End(0)))
        .map(|(inner, _pos)| TailReader { inner })
        .map(|reader| {
            FramedRead::new(
                BufReader::new(reader),
                LinesCodec::new_with_max_length(100 * 1024),
            )
        }).flatten_stream()
}

struct TailReader<T: AsyncRead> {
    inner: T,
}

impl<T: AsyncRead> Read for TailReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.inner.read(buf) {
            Ok(0) => Err(io::Error::new(io::ErrorKind::WouldBlock, "eof")),
            x => x,
        }
    }
}

impl<T: AsyncRead> AsyncRead for TailReader<T> {}

impl<T: AsyncRead> Drop for TailReader<T> {
    fn drop(&mut self) {
        info!("dropped!");
    }
}

type InnerSink = FramedWrite<BufWriter<File>, LinesCodec>;

pub struct Logg {
    data_dir: PathBuf,
    writer_state: WriterState,
    current_offset: usize,
    current_segment_size: usize,
}

enum WriterState {
    Writing(InnerSink),
    Rotating(CreateFuture<PathBuf>),
}

impl Logg {
    pub fn create(data_dir: &str) -> impl Future<Item = Self, Error = io::Error> {
        let data_dir = PathBuf::from(data_dir);
        let file = Self::create_segment_file(&data_dir, 0);
        file.map(Self::file_to_sink).map(|inner_sink| Self {
            data_dir,
            writer_state: WriterState::Writing(inner_sink),
            current_offset: 0,
            current_segment_size: 0,
        })
    }

    fn create_segment_file(dir: &Path, offset: usize) -> CreateFuture<PathBuf> {
        let filename = format!("{:020}.log", offset);
        let path = dir.join(filename);
        File::create(path)
    }

    fn file_to_sink(f: File) -> InnerSink {
        FramedWrite::new(
            BufWriter::new(f),
            LinesCodec::new_with_max_length(100 * 1024),
        )
    }
}

impl Sink for Logg {
    type SinkItem = String;
    type SinkError = io::Error;

    // TODO: topics and shutdown

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        loop {
            self.writer_state = match self.writer_state {
                WriterState::Rotating(_) => {
                    info!("sink not ready!");
                    return Ok(AsyncSink::NotReady(item));
                }
                WriterState::Writing(ref mut inner_sink) => {
                    self.current_offset += 1;
                    let new_segment_size = self.current_segment_size + item.len() + 4;
                    if new_segment_size < 64 * 1024 * 1024 {
                        self.current_segment_size = new_segment_size;
                        return inner_sink.start_send(item);
                    } else {
                        info!("rolling log segment!");
                        let mut create_fut =
                            Self::create_segment_file(&self.data_dir, self.current_offset);
                        match create_fut.poll() {
                            Ok(Async::Ready(file)) => {
                                info!("new file created!");
                                self.current_segment_size = 0;
                                WriterState::Writing(Self::file_to_sink(file))
                            }
                            x => {
                                info!("not ready {:?}", x);
                                WriterState::Rotating(create_fut)
                            }
                        }
                    }
                }
            };
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.writer_state = match self.writer_state {
            WriterState::Writing(ref mut inner_sink) => return inner_sink.poll_complete(),
            WriterState::Rotating(ref mut file_create) => match file_create.poll() {
                Ok(Async::Ready(file)) => {
                    info!("new file created!");
                    self.current_segment_size = 0;
                    WriterState::Writing(Self::file_to_sink(file))
                }
                x => {
                    info!("no new file yet! {:?}", x);
                    return Ok(().into());
                }
            },
        };
        Ok(Async::Ready(()))
    }
}
