//! For chunking.

/// A trait for implement chunking logic over a source frame `bytes`, into a variable number of frames.
pub trait Chunker {
    /// Chunks the input `bytes` frame into a variable number of frames.
    fn chunk(&self, bytes: bytes::Bytes) -> Result<Vec<bytes::Bytes>, vector_common::Error>;
}

/// Chunker implementations.
#[derive(Clone, Debug, Default)]
pub enum Chunkers {
    /// No chunking (pass-through).
    #[default]
    Noop,
    /// Chunking in GELF format.
    Gelf(crate::encoding::GelfChunker),
}

impl Chunker for Chunkers {
    fn chunk(&self, bytes: bytes::Bytes) -> Result<Vec<bytes::Bytes>, vector_common::Error> {
        match self {
            Chunkers::Noop => Ok(vec![bytes]),
            Chunkers::Gelf(chunker) => chunker.chunk(bytes),
        }
    }
}
