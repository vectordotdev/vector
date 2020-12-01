use super::InternalEvent;

#[derive(Debug)]
pub struct PulsarEncodeEventFailed<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for PulsarEncodeEventFailed<'a> {
    fn emit_logs(&self) {
        debug!(message = "Event encode failed.", error = ?self.error);
    }
}
