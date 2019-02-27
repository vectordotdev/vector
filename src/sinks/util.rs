pub mod batch;
pub mod http;
pub mod size_buffered;

use futures::Sink;

pub trait SinkExt: Sink<SinkItem = Vec<u8>> + Sized {
    fn size_buffered(self, limit: usize, gzip: bool) -> size_buffered::SizeBuffered<Self> {
        size_buffered::SizeBuffered::new(self, limit, gzip)
    }
}

impl<S> SinkExt for S where S: Sink<SinkItem = Vec<u8>> + Sized {}
