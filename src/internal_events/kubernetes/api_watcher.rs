use super::InternalEvent;
use std::fmt::Debug;

#[derive(Debug)]
pub struct RequestPrepared<R> {
    pub request: R,
}

impl<R: Debug> InternalEvent for RequestPrepared<R> {
    fn emit_logs(&self) {
        trace!(message = "request prepared", ?self.request);
    }
}

#[derive(Debug)]
pub struct ResponseReceived<R> {
    pub response: R,
}

impl<R: Debug> InternalEvent for ResponseReceived<R> {
    fn emit_logs(&self) {
        trace!(message = "got response", ?self.response);
    }
}
