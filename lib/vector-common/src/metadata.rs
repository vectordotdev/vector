/// Metadata for batch requests.
#[derive(Clone, Debug, Default)]
pub struct RequestMetadata {
    /// Number of events represented by this batch request.
    event_count: usize,
    /// Size, in bytes, of the in-memory representation of all events in this batch request.
    events_byte_size: usize,
    /// Uncompressed size, in bytes, of the encoded events in this batch request.
    request_encoded_size: usize,
    /// On-the-wire size, in bytes, of the batch request itself after compression, etc.
    ///
    /// This is akin to the bytes sent/received over the network, regardless of whether or not compression was used.
    request_wire_size: usize,
}

// TODO: Make this struct the object which emits the actual internal telemetry i.e. events sent, bytes sent, etc.
impl RequestMetadata {
    pub fn new(
        event_count: usize,
        events_byte_size: usize,
        request_encoded_size: usize,
        request_wire_size: usize,
    ) -> Self {
        Self {
            event_count,
            events_byte_size,
            request_encoded_size,
            request_wire_size,
        }
    }

    pub const fn event_count(&self) -> usize {
        self.event_count
    }

    pub const fn events_byte_size(&self) -> usize {
        self.events_byte_size
    }

    pub const fn request_encoded_size(&self) -> usize {
        self.request_encoded_size
    }

    pub const fn request_wire_size(&self) -> usize {
        self.request_wire_size
    }

    pub fn from_batch(metadata_vec: &Vec<RequestMetadata>) -> Self {
        let mut event_count = 0;
        let mut events_byte_size = 0;
        let mut request_encoded_size = 0;
        let mut request_wire_size = 0;

        for m in metadata_vec {
            event_count += m.event_count();
            events_byte_size += m.events_byte_size();
            request_encoded_size += m.request_encoded_size();
            request_wire_size += m.request_wire_size();
        }

        Self {
            event_count,
            events_byte_size,
            request_encoded_size,
            request_wire_size,
        }
    }
}

///
pub trait MetaDescriptive {
    fn get_metadata(&self) -> &RequestMetadata;
}
