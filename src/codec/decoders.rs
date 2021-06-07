use vector_core::event::Event;

pub use crate::config::decoding::{Decoding, DecodingConfig};

pub enum Decoder {
    Utf8,
    Json,
}

impl Decoder {
    pub fn decode(self, event: Event) -> Event {
        event
    }
}

impl From<DecodingConfig> for Decoder {
    fn from(config: DecodingConfig) -> Self {
        match config.0 {
            Decoding::Utf8 => Self::Utf8,
            Decoding::Json => Self::Json,
        }
    }
}
