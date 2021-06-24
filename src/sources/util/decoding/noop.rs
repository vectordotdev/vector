#![deny(missing_docs)]

use super::Decoder;
use crate::event::Value;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// A decoder which returns its input as-is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoopDecoder;

#[typetag::serde(name = "noop")]
impl Decoder for NoopDecoder {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn build(&self) -> crate::Result<Box<dyn Fn(Bytes) -> crate::Result<Value> + Send + Sync>> {
        Ok(Box::new(|bytes| Ok(bytes.into())))
    }
}

inventory::submit! {
    Box::new(NoopDecoder) as Box<dyn Decoder>
}
