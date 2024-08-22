// Structs used for our vector event logs

// Struct for vector send events (sending, uploaded)
#[derive(Clone, Debug)]
pub struct VectorSendEventMetadata {
    pub bytes: usize,
    pub events_len: usize,
    pub blob: String,
    pub container: String,
}

impl VectorSendEventMetadata {
    pub fn emit_upload_event(&self) {
        info!(
            message = "Uploaded events.",
            bytes = self.bytes,
            events_len = self.events_len,
            blob = self.blob,
            container = self.container,
            // VECTOR_UPLOADED_MESSAGES_EVENT
            vector_event_type = 4
        );
    }

    pub fn emit_sending_event(&self) {
        info!(
            message = "Sending events.",
            bytes = self.bytes,
            events_len = self.events_len,
            blob = self.blob,
            container = self.container,
            // VECTOR_SENDING_MESSAGES_EVENT
            vector_event_type = 3
        );
    }
}
