use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{
        ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage, error_type,
    },
};

/// Emitted when the rendered `tenant.id` is not a valid VictoriaMetrics tenant.
#[derive(Debug, NamedInternalEvent)]
pub struct VictoriaMetricsInvalidTenantError<'a> {
    pub tenant: &'a str,
}

impl InternalEvent for VictoriaMetricsInvalidTenantError<'_> {
    fn emit(self) {
        let reason = "Rendered tenant is not a valid VictoriaMetrics tenant.";
        error!(
            message = reason,
            tenant = %self.tenant,
            error_type = error_type::TEMPLATE_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::TEMPLATE_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}
