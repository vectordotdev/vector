use futures::Stream;
use log::error;
use tokio::codec::{FramedRead, LinesCodec};
use tokio::io::AsyncRead;

pub mod splunk;

pub fn reader_source<T: AsyncRead>(inner: T) -> impl Stream<Item = String, Error = ()> {
    FramedRead::new(inner, LinesCodec::new_with_max_length(100 * 1024))
        .map_err(|e| error!("error reading source: {:?}", e))
}
