use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};

#[derive(Debug, NamedInternalEvent)]
pub struct FileDescriptorReadError<E> {
    pub error: E,
}

impl<E> InternalEvent for FileDescriptorReadError<E>
where
    E: std::fmt::Display,
{
    fn emit(self) {
        error!(
            message = "Error reading from file descriptor.",
            error = %self.error,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}
