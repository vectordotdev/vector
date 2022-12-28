use bytes::{BufMut, BytesMut};

use crate::{codecs::Transformer, event::Event};

pub struct DatabendEventEncoder {
    pub(crate) transformer: Transformer,
}

impl DatabendEventEncoder {
    pub(crate) fn encode_event(&mut self, mut event: Event) -> BytesMut {
        self.transformer.transform(&mut event);
        let log = event.into_log();

        let mut content =
            crate::serde::json::to_bytes(&log).expect("Failed to encode event as JSON.");
        content.put_u8(b'\n');

        content
    }
}
