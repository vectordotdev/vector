//! Optional extension for encoding formats to support chunking, used when the sink transport (e.g., UDP) has a payload size limit.
//!
//! Chunking allows large encoded events to be split into smaller frames, ensuring compatibility with transports that cannot send large payloads in a single datagram or packet.

mod gelf;

use bytes::Bytes;
pub use gelf::GelfChunker;

/// Trait for encoding formats that optionally support chunking, for use with sinks that have payload size limits (such as UDP).
///
/// Chunking is an extension to the standard `Encoder` trait, allowing large encoded events to be split into multiple frames for transmission.
pub trait Chunking {
    /// Chunks the input into frames.
    fn chunk(&self, bytes: Bytes) -> Result<Vec<Bytes>, vector_common::Error>;
}

/// Implementations of chunking strategies for supported formats.
#[derive(Clone, Debug)]
pub enum Chunker {
    /// GELF chunking implementation.
    Gelf(GelfChunker),
}

impl Chunking for Chunker {
    fn chunk(&self, bytes: Bytes) -> Result<Vec<Bytes>, vector_common::Error> {
        match self {
            Chunker::Gelf(chunker) => chunker.chunk(bytes),
        }
    }
}
