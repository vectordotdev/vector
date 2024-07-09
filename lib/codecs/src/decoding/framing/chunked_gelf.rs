use crate::BytesDecoder;

use super::BoxedFramingError;
use bytes::{Buf, Bytes, BytesMut};
use derivative::Derivative;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio;
use tokio::task::JoinHandle;
use tokio_util::codec::Decoder;
use tracing::{info, warn};
use vector_config::configurable_component;

const GELF_MAGIC: [u8; 2] = [0x1e, 0x0f];
pub const MAX_TOTAL_CHUNKS: u8 = 128;
const DEFAULT_CHUNKS: [Bytes; MAX_TOTAL_CHUNKS as usize] =
    [const { Bytes::new() }; MAX_TOTAL_CHUNKS as usize];
const DEFAULT_TIMEOUT_MILLIS: u64 = 5000;
const DEFAULT_PENDING_MESSAGES_LIMIT: usize = 1000;

const fn default_timeout_millis() -> u64 {
    DEFAULT_TIMEOUT_MILLIS
}

const fn default_pending_messages_limit() -> usize {
    DEFAULT_PENDING_MESSAGES_LIMIT
}

/// Config used to build a `ChunkedGelfDecoderConfig`.
#[configurable_component]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkedGelfDecoderConfig {
    /// Options for the chunked gelf decoder.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub chunked_gelf: ChunkedGelfDecoderOptions,
}

impl ChunkedGelfDecoderConfig {
    /// Creates a new `BytesDecoderConfig`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Build the `ByteDecoder` from this configuration.
    pub fn build(&self) -> ChunkedGelfDecoder {
        ChunkedGelfDecoder::new(
            self.chunked_gelf.timeout_millis,
            self.chunked_gelf.pending_messages_limit,
        )
    }
}

/// Options for building a `ChunkedGelfDecoder`.
#[configurable_component]
#[derive(Clone, Debug, Derivative, PartialEq, Eq)]
pub struct ChunkedGelfDecoderOptions {
    /// The timeout in milliseconds for a message to be fully received. If the timeout is reached, the
    /// decoder will drop all the received chunks of the message and start over.
    #[serde(
        default = "default_timeout_millis",
        skip_serializing_if = "vector_core::serde::is_default"
    )]
    pub timeout_millis: u64,

    /// The maximum number of pending uncomplete messages. If this limit is reached, the decoder will start
    /// dropping chunks of new messages. This limit ensures the memory usage of the decoder is bounded.
    #[serde(
        default = "default_pending_messages_limit",
        skip_serializing_if = "vector_core::serde::is_default"
    )]
    pub pending_messages_limit: usize,
}

impl Default for ChunkedGelfDecoderOptions {
    fn default() -> Self {
        Self {
            timeout_millis: default_timeout_millis(),
            pending_messages_limit: default_pending_messages_limit(),
        }
    }
}

#[derive(Debug)]
pub struct MessageState {
    total_chunks: u8,
    chunks: [Bytes; MAX_TOTAL_CHUNKS as usize],
    chunks_bitmap: u128,
    timeout_task: JoinHandle<()>,
}

impl MessageState {
    pub fn new(total_chunks: u8, timeout_task: JoinHandle<()>) -> Self {
        Self {
            total_chunks,
            chunks: DEFAULT_CHUNKS,
            chunks_bitmap: 0,
            timeout_task,
        }
    }

    pub fn is_chunk_present(&self, sequence_number: u8) -> bool {
        let chunk_bitmap_id = 1 << sequence_number;
        self.chunks_bitmap & chunk_bitmap_id != 0
    }

    pub fn add_chunk(&mut self, sequence_number: u8, chunk: Bytes) {
        let chunk_bitmap_id = 1 << sequence_number;
        self.chunks[sequence_number as usize] = chunk;
        self.chunks_bitmap |= chunk_bitmap_id;
    }

    pub fn is_complete(&self) -> bool {
        self.chunks_bitmap.count_ones() == self.total_chunks as u32
    }

    pub fn retrieve_message(&mut self) -> Option<Bytes> {
        if self.is_complete() {
            self.timeout_task.abort();
            let chunks = &self.chunks[0..self.total_chunks as usize];
            let mut message = BytesMut::new();
            for chunk in chunks {
                message.extend_from_slice(chunk);
            }
            Some(message.freeze())
        } else {
            None
        }
    }
}

/// A decoder for handling GELF messages that may be chunked.
#[derive(Debug, Clone)]
pub struct ChunkedGelfDecoder {
    // We have to use this decoder to read all the bytes from the buffer first and don't let tokio
    // read it buffered, as tokio FramedRead will not always call the decode method with the
    // whole message. (see https://docs.rs/tokio-util/latest/src/tokio_util/codec/framed_impl.rs.html#26).
    // This limitation is due to the fact that the GELF format does not specify the length of the
    // message, so we have to read all the bytes from the message (datagram)
    inner_decoder: BytesDecoder,
    state: Arc<Mutex<HashMap<u64, MessageState>>>,
    timeout: Duration,
    pending_messages_limit: usize,
}

impl ChunkedGelfDecoder {
    /// Creates a new `ChunkedGelfDecoder`.
    pub fn new(timeout_millis: u64, pending_messages_limit: usize) -> Self {
        Self {
            inner_decoder: BytesDecoder::new(),
            state: Arc::new(Mutex::new(HashMap::new())),
            timeout: Duration::from_millis(timeout_millis),
            pending_messages_limit,
        }
    }

    /// TODO: document this
    pub fn decode_chunk(&mut self, mut chunk: Bytes) -> Result<Option<Bytes>, BoxedFramingError> {
        // We need 10 bits to read the message id, sequence number and total chunks
        if chunk.remaining() < 10 {
            let src_display = format!("{chunk:?}");
            warn!(message = "Received malformed chunk headers (message ID, sequence number and total chunks) with less than 10 bytes. Ignoring it.",
                src = src_display,
                remaining = chunk.remaining(),
                internal_log_rate_limit = true
            );
            return Ok(None);
        }
        let message_id = chunk.get_u64();
        let sequence_number = chunk.get_u8();
        let total_chunks = chunk.get_u8();

        if total_chunks == 0 || total_chunks > MAX_TOTAL_CHUNKS {
            warn!(
                message = "Received a chunk with an invalid total chunks value. Ignoring it.",
                message_id = message_id,
                sequence_number = sequence_number,
                total_chunks = total_chunks,
                internal_log_rate_limit = true
            );
            return Ok(None);
        }

        if sequence_number >= total_chunks {
            warn!(
                message = "Received a chunk with a sequence number greater than total chunks. Ignoring it.",
                message_id = message_id,
                sequence_number = sequence_number,
                total_chunks = total_chunks,
                internal_log_rate_limit = true
            );
            return Ok(None);
        }

        let mut state_lock = self.state.lock().unwrap();

        if state_lock.len() >= self.pending_messages_limit {
            warn!(
                message = "Received a chunk but reached the pending messages limit. Ignoring it.",
                message_id = message_id,
                sequence_number = sequence_number,
                total_chunks = total_chunks,
                pending_messages_limit = self.pending_messages_limit,
                internal_log_rate_limit = true
            );
            return Ok(None);
        }

        let message_state = state_lock.entry(message_id).or_insert_with(|| {
            // We need to spawn a task that will clear the message state after a certain time
            // otherwise we will have a memory leak due to messages that never complete
            let state = Arc::clone(&self.state);
            let timeout = self.timeout.clone();
            let timeout_handle = tokio::spawn(async move {
                tokio::time::sleep(timeout).await;
                let mut state_lock = state.lock().unwrap();
                if let Some(_) = state_lock.remove(&message_id) {
                    // TODO: log the variables in the message or use structured logging?
                    // TODO: record metrics here? it would be insteresting to know how many messages are being discarded
                    warn!(
                        message = "Message was not fully received within the timeout window. Discarding it.",
                        message_id = message_id,
                        timeout = timeout.as_millis(),
                        internal_log_rate_limit = true
                    );
                }
            });
            MessageState::new(total_chunks, timeout_handle)
        });

        if message_state.total_chunks != total_chunks {
            warn!(message_id = "Received a chunk with a different total chunks than the original. Ignoring it.",
                message_id = message_id
                original_total_chunks = message_state.total_chunks,
                received_total_chunks = total_chunks,
                internal_log_rate_limit = true,
            );
            return Ok(None);
        }

        if message_state.is_chunk_present(sequence_number) {
            // TODO: add a PR comment asking for info or warn
            info!(
                message = "Received a duplicate chunk. Ignoring it.",
                sequence_number = sequence_number,
                message_id = message_id,
                internal_log_rate_limit = true
            );
            return Ok(None);
        }

        message_state.add_chunk(sequence_number, chunk);

        if let Some(message) = message_state.retrieve_message() {
            state_lock.remove(&message_id);
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    /// TODO: document this
    pub fn decode_message(&mut self, mut src: Bytes) -> Result<Option<Bytes>, BoxedFramingError> {
        let magic = src.get(0..2);
        if magic.is_some_and(|magic| magic == GELF_MAGIC) {
            src.advance(2);
            let frame = self.decode_chunk(src);
            return frame;
        } else {
            return Ok(Some(src));
        }
    }
}

impl Default for ChunkedGelfDecoder {
    fn default() -> Self {
        Self::new(DEFAULT_TIMEOUT_MILLIS, DEFAULT_PENDING_MESSAGES_LIMIT)
    }
}

impl Decoder for ChunkedGelfDecoder {
    type Item = Bytes;

    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        // TODO: add a PR comment here stating that this will never call the decode_message since
        // the bytes decoder will always return a Ok(None) in this method, but leaving this
        // here for consistency. Would be better to add a unreachable/panic here if the inner decoder returns
        // the Some variant?
        self.inner_decoder
            .decode(src)?
            .and_then(|frame| self.decode_message(frame).transpose())
            .transpose()
    }
    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if buf.is_empty() {
            return Ok(None);
        }

        self.inner_decoder
            .decode_eof(buf)?
            .and_then(|frame| self.decode_message(frame).transpose())
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{BufMut, BytesMut};
    use rstest::{fixture, rstest};
    use tracing_test::traced_test;

    fn create_chunk(
        message_id: u64,
        sequence_number: u8,
        total_chunks: u8,
        payload: &str,
    ) -> BytesMut {
        let mut chunk = BytesMut::new();
        chunk.put_slice(&GELF_MAGIC);
        chunk.put_u64(message_id);
        chunk.put_u8(sequence_number);
        chunk.put_u8(total_chunks);
        chunk.extend_from_slice(payload.as_bytes());
        chunk
    }

    #[fixture]
    fn unchunked_message() -> (BytesMut, String) {
        let payload = "foo";
        (BytesMut::from(payload), payload.to_string())
    }

    // TODO: add a malformed chunk message

    #[fixture]
    fn two_chunks_message() -> ([BytesMut; 2], String) {
        let message_id = 1u64;
        let total_chunks = 2u8;

        let first_sequence_number = 0u8;
        let first_payload = "foo";
        let first_chunk = create_chunk(
            message_id,
            first_sequence_number,
            total_chunks,
            first_payload,
        );

        let second_sequence_number = 1u8;
        let second_payload = "bar";
        let second_chunk = create_chunk(
            message_id,
            second_sequence_number,
            total_chunks,
            second_payload,
        );

        (
            [first_chunk, second_chunk],
            format!("{first_payload}{second_payload}"),
        )
    }

    #[fixture]
    fn three_chunks_message() -> ([BytesMut; 3], String) {
        let message_id = 2u64;
        let total_chunks = 3u8;

        let first_sequence_number = 0u8;
        let first_payload = "foo";
        let first_chunk = create_chunk(
            message_id,
            first_sequence_number,
            total_chunks,
            first_payload,
        );

        let second_sequence_number = 1u8;
        let second_payload = "bar";
        let second_chunk = create_chunk(
            message_id,
            second_sequence_number,
            total_chunks,
            second_payload,
        );

        let third_sequence_number = 2u8;
        let third_payload = "baz";
        let third_chunk = create_chunk(
            message_id,
            third_sequence_number,
            total_chunks,
            third_payload,
        );

        (
            [first_chunk, second_chunk, third_chunk],
            format!("{first_payload}{second_payload}{third_payload}"),
        )
    }

    #[rstest]
    #[tokio::test]
    async fn decode_chunked(two_chunks_message: ([BytesMut; 2], String)) {
        let (mut chunks, expected_message) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut chunks[1]).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_message)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_unchunked(unchunked_message: (BytesMut, String)) {
        let (mut message, expected_message) = unchunked_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut message).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_message)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_unordered_chunks(two_chunks_message: ([BytesMut; 2], String)) {
        let (mut chunks, expected_message) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunks[1]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_message)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_unordered_messages(
        two_chunks_message: ([BytesMut; 2], String),
        three_chunks_message: ([BytesMut; 3], String),
    ) {
        let (mut two_chunks, two_chunks_expected) = two_chunks_message;
        let (mut three_chunks, three_chunks_expected) = three_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut three_chunks[2]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut two_chunks[0]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut three_chunks[0]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut two_chunks[1]).unwrap();
        assert_eq!(frame, Some(Bytes::from(two_chunks_expected)));

        let frame = decoder.decode_eof(&mut three_chunks[1]).unwrap();
        assert_eq!(frame, Some(Bytes::from(three_chunks_expected)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_mixed_chunked_and_unchunked_messages(
        unchunked_message: (BytesMut, String),
        two_chunks_message: ([BytesMut; 2], String),
    ) {
        let (mut unchunked_message, expected_unchunked_message) = unchunked_message;
        let (mut chunks, expected_chunked_message) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunks[1]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut unchunked_message).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_unchunked_message)));

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_chunked_message)));
    }

    #[rstest]
    #[tokio::test(start_paused = true)]
    #[traced_test]
    async fn decode_timeout(two_chunks_message: ([BytesMut; 2], String)) {
        let timeout = 300;
        let (mut chunks, _) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::new(timeout, DEFAULT_PENDING_MESSAGES_LIMIT);

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert!(frame.is_none());
        assert!(!decoder.state.lock().unwrap().is_empty());

        // The message state should be cleared after a certain time
        tokio::time::sleep(Duration::from_millis(timeout + 1)).await;
        assert!(decoder.state.lock().unwrap().is_empty());
        assert!(logs_contain(
            "Message was not fully received within the timeout window. Discarding it."
        ));

        let frame = decoder.decode_eof(&mut chunks[1]).unwrap();
        assert!(frame.is_none());

        tokio::time::sleep(Duration::from_millis(timeout + 1)).await;
        assert!(decoder.state.lock().unwrap().is_empty());
        assert!(logs_contain(
            "Message was not fully received within the timeout window. Discarding it."
        ));
    }

    #[tokio::test]
    async fn decode_empty_input() {
        let mut src = BytesMut::new();
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut src).unwrap();
        assert!(frame.is_none());
    }

    #[tokio::test]
    #[traced_test]
    async fn decode_chunk_with_malformed_header() {
        let malformed_chunk = [0x12, 0x23];
        let mut src = BytesMut::new();
        src.extend_from_slice(&GELF_MAGIC);
        src.extend_from_slice(&malformed_chunk);
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut src).unwrap();
        assert!(frame.is_none());
        assert!(logs_contain("Received malformed chunk headers (message ID, sequence number and total chunks) with less than 10 bytes. Ignoring it."));
    }

    #[tokio::test]
    #[traced_test]
    async fn decode_chunk_with_invalid_total_chunks() {
        let message_id = 1u64;
        let sequence_number = 1u8;
        let invalid_total_chunks = MAX_TOTAL_CHUNKS + 1;
        let payload = "foo";
        let mut chunk = create_chunk(message_id, sequence_number, invalid_total_chunks, payload);
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunk).unwrap();
        assert!(frame.is_none());
        assert!(logs_contain(
            "Received a chunk with an invalid total chunks value. Ignoring it."
        ));
    }

    #[tokio::test]
    #[traced_test]
    async fn decode_chunk_with_invalid_sequence_number() {
        let message_id = 1u64;
        let total_chunks = 2u8;
        let invalid_sequence_number = total_chunks + 1;
        let payload = "foo";
        let mut chunk = create_chunk(message_id, invalid_sequence_number, total_chunks, payload);
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunk).unwrap();
        assert!(frame.is_none());
        assert!(logs_contain(
            "Received a chunk with a sequence number greater than total chunks. Ignoring it."
        ));
    }

    #[rstest]
    #[tokio::test]
    #[traced_test]
    async fn decode_when_reached_pending_messages_limit(
        two_chunks_message: ([BytesMut; 2], String),
        three_chunks_message: ([BytesMut; 3], String),
    ) {
        let pending_messages_limit = 1;
        let (mut two_chunks, _) = two_chunks_message;
        let (mut three_chunks, _) = three_chunks_message;
        let mut decoder = ChunkedGelfDecoder::new(DEFAULT_TIMEOUT_MILLIS, pending_messages_limit);

        let frame = decoder.decode_eof(&mut two_chunks[0]).unwrap();
        assert!(frame.is_none());
        assert!(decoder.state.lock().unwrap().len() == 1);

        let frame = decoder.decode_eof(&mut three_chunks[0]).unwrap();
        assert!(frame.is_none());
        assert!(decoder.state.lock().unwrap().len() == 1);
        assert!(logs_contain(
            "Received a chunk but reached the pending messages limit. Ignoring it."
        ));
    }

    #[rstest]
    #[tokio::test]
    #[traced_test]
    async fn decode_chunk_with_different_total_chunks() {
        let message_id = 1u64;
        let sequence_number = 0u8;
        let total_chunks = 2u8;
        let payload = "foo";
        let mut first_chunk = create_chunk(message_id, sequence_number, total_chunks, payload);
        let mut second_chunk =
            create_chunk(message_id, sequence_number + 1, total_chunks + 1, payload);
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut first_chunk).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut second_chunk).unwrap();
        assert!(frame.is_none());
        assert!(logs_contain(
            "Received a chunk with a different total chunks than the original. Ignoring it."
        ));
    }

    #[rstest]
    #[tokio::test]
    #[traced_test]
    async fn decode_when_duplicated_chunk(two_chunks_message: ([BytesMut; 2], String)) {
        let (mut chunks, _) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunks[0].clone()).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert!(frame.is_none());
        assert!(logs_contain("Received a duplicate chunk. Ignoring it."));
    }
}
