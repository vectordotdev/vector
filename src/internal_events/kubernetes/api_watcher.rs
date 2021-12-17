use std::fmt::Debug;

use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct RequestPrepared<R> {
    pub request: R,
}

impl<R: Debug> InternalEvent for RequestPrepared<R> {
    fn emit_logs(&self) {
        trace!(message = "Request prepared.", request = ?self.request);
    }
}

#[derive(Debug)]
pub struct ResponseReceived<R> {
    pub response: R,
}

impl<R: Debug> InternalEvent for ResponseReceived<R> {
    fn emit_logs(&self) {
        trace!(message = "Got response.", response = ?self.response);
    }
}
