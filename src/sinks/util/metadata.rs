use super::request_builder::EncodeResult;

/// Metadata for batch requests.
#[derive(Clone, Debug)]
pub struct BatchRequestMetadata {
    /// Number of events represented by this batch request.
    pub event_count: usize,
    /// Size, in bytes, of the in-memory representation of all events in this batch request.
    pub event_byte_size: usize,
    /// Uncompressed size, in bytes, of the encoded events in this batch request.
    pub encoded_uncompressed_size: usize,
    /// Compressed size, in bytes, of the encoded events in this batch request, if compression was performed.
    pub encoded_compressed_size: Option<usize>,
}

// TODO: Make this struct the object which emits the actual internal telemetry i.e. events sent, bytes sent, etc.
impl BatchRequestMetadata {
    pub fn new<T>(
        event_count: usize,
        event_byte_size: usize,
        encode_result: &EncodeResult<T>,
    ) -> Self {
        Self {
            event_count,
            event_byte_size,
            encoded_uncompressed_size: encode_result.uncompressed_byte_size,
            encoded_compressed_size: encode_result.compressed_byte_size,
        }
    }
}
