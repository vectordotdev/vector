use metrics::counter;
use vector_lib::internal_event::{error_stage, error_type};
use vector_lib::internal_event::{ComponentEventsDropped, InternalEvent, UNINTENTIONAL};

pub struct PairExpansionError<'a> {
    pub key: &'a str,
    pub value: &'a str,
    pub drop_event: bool,
    pub error: serde_json::Error,
}

impl InternalEvent for PairExpansionError<'_> {
    fn emit(self) {
        let message = format!("Failed to expand key: `{}`:`{}`", self.key, self.value);

        if self.drop_event {
            error!(
                message = %message,
                error = %self.error,
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );

            counter!(
                "component_errors_total",
                "error_type" => error_type::PARSER_FAILED,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);

            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: &message,
            });
        } else {
            warn!(
                message = %message,
                error = %self.error,
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
        }
    }
}
