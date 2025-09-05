//! A collection of formats that can be used to chunk events into multiple byte frames.

mod gelf;

use bytes::Bytes;
pub use gelf::GelfChunker;

/// For chunking.
pub trait Chunking {
    /// Chunks the input into frames.
    fn chunk(&self, bytes: Bytes) -> Result<Vec<Bytes>, vector_common::Error>;
}

/// Chunking implementations.
#[derive(Clone, Debug)]
pub enum Chunker {
    /// Chunking in GELF format.
    Gelf(GelfChunker),
}

impl Chunking for Chunker {
    fn chunk(&self, bytes: Bytes) -> Result<Vec<Bytes>, vector_common::Error> {
        match self {
            Chunker::Gelf(chunker) => chunker.chunk(bytes),
        }
    }
}
