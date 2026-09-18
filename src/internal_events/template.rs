use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{
        ComponentEventsDropped, CounterName, INTENTIONAL, InternalEvent, UNINTENTIONAL,
        error_stage, error_type,
    },
};

#[derive(NamedInternalEvent)]
pub struct TemplateRenderingError<'a> {
    pub field: Option<&'a str>,
    pub drop_event: bool,
    pub error: crate::template::TemplateRenderingError,
}

impl InternalEvent for TemplateRenderingError<'_> {
    fn emit(self) {
        let confined = matches!(
            self.error,
            crate::template::TemplateRenderingError::Confined { .. }
        );

        // Message wording tracks BOTH the error class (confinement vs render
        // failure) AND whether the caller is dropping the event. A caller
        // like the KeyPartitioner falling back to a dead-letter key emits
        // `Confined` + `drop_event: false` — we still want to log the
        // confinement violation, but claiming "dropping event" would be a
        // lie because the event still routes to the dead-letter.
        let mut msg = match (confined, self.drop_event) {
            (true, true) => {
                "Templated routing value was outside the configured confinement; dropping event"
                    .to_owned()
            }
            (true, false) => {
                "Templated routing value was outside the configured confinement".to_owned()
            }
            (false, _) => "Failed to render template".to_owned(),
        };
        if let Some(field) = self.field {
            use std::fmt::Write;
            _ = write!(msg, " for \"{field}\"");
        }
        msg.push('.');

        // A `Confined` error is always alert-worthy: an attacker attempted
        // to steer routing via event data. Some callers legitimately don't
        // drop the event (e.g. a partitioner falling back to a dead-letter
        // key), but the confinement fire itself must still surface in logs
        // and `component_errors_total` regardless of the caller's
        // drop_event decision.
        //
        // `check-events` requires `error_type` counter tag values to be
        // constants, so the two error-class branches are split
        // explicitly. Confined renders always surface at `error!` +
        // `component_errors_total` (they're security-relevant even when
        // the caller doesn't drop the event); non-confined renders only
        // surface on the drop path.
        if confined {
            error!(
                message = %msg,
                error = %self.error,
                error_type = error_type::CONFINEMENT_FAILED,
                stage = error_stage::PROCESSING,
            );
            counter!(
                CounterName::ComponentErrorsTotal,
                "error_type" => error_type::CONFINEMENT_FAILED,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
        } else if self.drop_event {
            error!(
                message = %msg,
                error = %self.error,
                error_type = error_type::TEMPLATE_FAILED,
                stage = error_stage::PROCESSING,
            );
            counter!(
                CounterName::ComponentErrorsTotal,
                "error_type" => error_type::TEMPLATE_FAILED,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
        } else {
            warn!(
                message = %msg,
                error = %self.error,
                error_type = error_type::TEMPLATE_FAILED,
                stage = error_stage::PROCESSING,
            );
        }

        // Only emit `ComponentEventsDropped` when the caller actually
        // dropped the event. `Confined` + `drop_event: false` (e.g. the
        // dead-letter fallback) doesn't count as a drop — the event still
        // lands somewhere, just at the operator-authored fallback key.
        if self.drop_event {
            if confined {
                emit!(ComponentEventsDropped::<INTENTIONAL> {
                    count: 1,
                    reason: "Template rendered a value outside the confined base.",
                });
            } else {
                emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                    count: 1,
                    reason: "Failed to render template.",
                });
            }
        }
    }
}
