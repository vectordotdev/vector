#![allow(dead_code)] // TODO requires optional feature compilation

use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{
        ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage, error_type,
    },
};

#[derive(Debug, NamedInternalEvent)]
pub struct IggySendError<'a> {
    pub count: usize,
    pub error: &'a iggy::prelude::IggyError,
}

impl InternalEvent for IggySendError<'_> {
    fn emit(self) {
        let reason = "Failed to send messages to Iggy.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason,
        });
    }
}
