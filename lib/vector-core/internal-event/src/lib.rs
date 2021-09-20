//! The Vector Core Internal Event library
//!
//! This library powers the Event-driven Observability pattern (RFC 2064) that
//! vector uses for internal instrumentation

#![deny(clippy::all)]
#![deny(clippy::pedantic)]

pub trait InternalEvent {
    fn emit_logs(&self) {}
    fn emit_metrics(&self) {}
}

pub fn emit(event: impl InternalEvent) {
    event.emit_logs();
    event.emit_metrics();
}
