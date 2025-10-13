use std::vec;

use super::Chunking;
use bytes::{BufMut, Bytes, BytesMut};
use tracing::trace;

const GELF_MAX_TOTAL_CHUNKS: usize = 128;
const GELF_CHUNK_HEADERS_LENGTH: usize = 12;
const GELF_MAGIC_BYTES: [u8; 2] = [0x1e, 0x0f];

/// Chunks with GELF native chunking format, as documented from the [source][source].
/// Supports up to 128 chunks, each with a maximum size that can be configured.
///
/// [source]: https://go2docs.graylog.org/current/getting_in_log_data/gelf.html#chunking
#[derive(Clone, Debug)]
pub struct GelfChunker {
    /// Max chunk size. This must be at least 13 bytes (12 bytes for headers + N bytes for data).
    /// There is no specific upper limit, since it depends on the transport protocol and network interface settings.
    /// Most networks will limit IP frames to 64KiB; however, the actual payload size limit will be lower due to UDP and GELF headers.
    ///
    /// For safety it is not recommended to set this value any higher than 65,500 bytes unless your network supports [Jumbograms][jumbogram].
    ///
    /// [jumbogram]: https://en.wikipedia.org/wiki/Jumbogram
    pub max_chunk_size: usize,
}

impl Chunking for GelfChunker {
    fn chunk(&self, bytes: Bytes) -> Result<Vec<Bytes>, vector_common::Error> {
        if bytes.len() <= self.max_chunk_size {
            return Ok(vec![bytes]);
        }

        let chunk_size = self.max_chunk_size - GELF_CHUNK_HEADERS_LENGTH;
        let message_id: u64 = rand::random();
        let chunk_count = bytes.len().div_ceil(chunk_size);

        trace!(
            message_id = message_id,
            chunk_count = chunk_count,
            chunk_size = chunk_size,
            "Generating chunks for GELF."
        );

        if chunk_count > GELF_MAX_TOTAL_CHUNKS {
            return Err(vector_common::Error::from(format!(
                "Too many chunks to generate for GELF: {}, max: {}",
                chunk_count, GELF_MAX_TOTAL_CHUNKS
            )));
        }

        // Split into chunks and add headers to each slice.
        // Map with index to determine sequence number.
        let chunks = bytes
            .chunks(chunk_size)
            .enumerate()
            .map(|(i, chunk)| {
                let framed = Bytes::copy_from_slice(chunk);
                let sequence_number = i as u8;
                let sequence_count = chunk_count as u8;

                let mut headers = BytesMut::with_capacity(GELF_CHUNK_HEADERS_LENGTH);
                headers.put_slice(&GELF_MAGIC_BYTES);
                headers.put_u64(message_id);
                headers.put_u8(sequence_number);
                headers.put_u8(sequence_count);

                [headers.freeze(), framed].concat().into()
            })
            .collect();
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::{Chunking, GELF_CHUNK_HEADERS_LENGTH, GELF_MAGIC_BYTES, GelfChunker};
    use crate::encoding::Chunker;

    #[test]
    fn test_gelf_chunker_noop() {
        let chunker = Chunker::Gelf(GelfChunker {
            max_chunk_size: 8192,
        });
        let input = Bytes::from("1234123412341234123");
        let chunks = chunker.chunk(input.clone()).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], input);
    }

    #[test]
    fn test_gelf_chunker_chunk() {
        let chunker = Chunker::Gelf(GelfChunker {
            max_chunk_size: GELF_CHUNK_HEADERS_LENGTH + 4,
        });
        // Input for 5 chunks of 4 bytes: [1234] [1234] [1234] [1234] [123]
        let input = Bytes::from("1234123412341234123");
        let chunks = chunker.chunk(input).unwrap();
        assert_eq!(chunks.len(), 5);

        for i in 0..chunks.len() {
            if i < 4 {
                assert_eq!(chunks[i].len(), GELF_CHUNK_HEADERS_LENGTH + 4);
            } else {
                assert_eq!(chunks[i].len(), GELF_CHUNK_HEADERS_LENGTH + 3);
            }
            // Bytes 0 and 1: Magic bytes
            assert_eq!(chunks[i][0..2], GELF_MAGIC_BYTES);
            // Bytes 2 to 9: Random ID (not checked)
            // Byte 10: Sequence number
            assert_eq!(chunks[i][10], i as u8);
            // Byte 11: Sequence count
            assert_eq!(chunks[i][11], chunks.len() as u8);
            // Payload bytes
            if i < 4 {
                assert_eq!(&chunks[i][GELF_CHUNK_HEADERS_LENGTH..], b"1234");
            } else {
                assert_eq!(&chunks[i][GELF_CHUNK_HEADERS_LENGTH..], b"123");
            }
        }
    }

    #[test]
    fn test_gelf_chunker_max() {
        let chunker = Chunker::Gelf(GelfChunker {
            max_chunk_size: GELF_CHUNK_HEADERS_LENGTH + 65500,
        });
        // Input for 128 chunks of 65500 bytes of data
        let input = Bytes::from_static(&[0; 65500 * 128]);
        let chunks = chunker.chunk(input).unwrap();
        assert_eq!(chunks.len(), 128);

        for i in 0..chunks.len() {
            assert_eq!(chunks[i].len(), GELF_CHUNK_HEADERS_LENGTH + 65500);
            // Bytes 0 and 1: Magic bytes
            assert_eq!(chunks[i][0..2], GELF_MAGIC_BYTES);
            // Bytes 2 to 9: Random ID (not checked)
            // Byte 10: Sequence number
            assert_eq!(chunks[i][10], i as u8);
            // Byte 11: Sequence count
            assert_eq!(chunks[i][11], chunks.len() as u8);
            // Payload bytes
            assert_eq!(&chunks[i][GELF_CHUNK_HEADERS_LENGTH..], &[0; 65500]);
        }
    }
}
