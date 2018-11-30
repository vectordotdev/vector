use futures::Stream;
use log::error;
use stream_cancel::Tripwire;
use tokio::codec::{FramedRead, LinesCodec};
use tokio::io::AsyncRead;
use Record;

pub mod splunk;

type Source = Box<dyn Stream<Item = Record, Error = ()> + Send>;

pub trait SourceFactory {
    type Config;

    fn build(config: Self::Config, exit: Tripwire) -> Source;
}

pub fn reader_source<T: AsyncRead>(inner: T) -> impl Stream<Item = Record, Error = ()> {
    FramedRead::new(inner, LinesCodec::new_with_max_length(100 * 1024))
        .map(Record::new_from_line)
        .map_err(|e| error!("error reading source: {:?}", e))
}
