use std::sync::Arc;

use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, Finalizable},
    ByteSizeOf,
};

use crate::sinks::util::ElementCount;

#[derive(Clone, Debug)]
pub struct HecRequest {
    pub body: Vec<u8>,
    pub events_count: usize,
    pub events_byte_size: usize,
    pub finalizers: EventFinalizers,
    pub passthrough_token: Option<Arc<str>>,
}

impl ByteSizeOf for HecRequest {
    fn allocated_bytes(&self) -> usize {
        self.body.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for HecRequest {
    fn element_count(&self) -> usize {
        self.events_count
    }
}

impl Ackable for HecRequest {
    fn ack_size(&self) -> usize {
        self.events_count
    }
}

impl Finalizable for HecRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}
