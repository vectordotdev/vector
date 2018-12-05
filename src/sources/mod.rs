use futures::{sync::mpsc, Future, Stream};
use log::error;
use tokio::codec::{FramedRead, LinesCodec};
use tokio::io::AsyncRead;
use Record;

pub mod splunk;

pub trait SourceFactory {
    type Config: Clone + Send;

    fn build(
        config: Self::Config,
        out: mpsc::Sender<Record>,
    ) -> Box<dyn Future<Item = (), Error = ()> + Send>;
}

pub fn reader_source<T: AsyncRead>(inner: T) -> impl Stream<Item = Record, Error = ()> {
    FramedRead::new(inner, LinesCodec::new_with_max_length(100 * 1024))
        .map(Record::new_from_line)
        .map_err(|e| error!("error reading source: {:?}", e))
}
