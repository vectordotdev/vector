use futures::{Future, Stream};
use log::error;
use tokio::codec::{FramedRead, LinesCodec};
use tokio::io::AsyncRead;
use crate::record::Record;

pub type Source = Box<dyn Future<Item = (), Error = ()> + Send>;

pub mod splunk;

pub fn reader_source<T: AsyncRead>(inner: T) -> impl Stream<Item = Record, Error = ()> {
    FramedRead::new(inner, LinesCodec::new_with_max_length(100 * 1024))
        .map(Record::new_from_line)
        .map_err(|e| error!("error reading source: {:?}", e))
}
