mod size_buffered;
pub mod http;

use futures::Sink;

pub trait SinkExt: Sink<SinkItem = Vec<u8>> + Sized {
    fn size_buffered(self, limit: usize) -> size_buffered::SizeBuffered<Self> {
        size_buffered::SizeBuffered::new(self, limit)
    }
}

impl<S> SinkExt for S where S: Sink<SinkItem = Vec<u8>> + Sized {}
