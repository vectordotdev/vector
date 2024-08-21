mod component_spec;

pub use self::component_spec::ComponentSpecValidator;

use std::fmt::{Display, Formatter};

use vector_lib::event::Event;

use super::{ComponentType, RunnerMetrics, TestCaseExpectation, TestEvent};

/// A component validator.
///
/// Validators perform the actual validation logic that, based on the given inputs, determine of the
/// component is valid or not for the given validator.
pub trait Validator {
    /// Gets the unique name of this validator.
    fn name(&self) -> &'static str;

    /// Processes the given set of inputs/outputs, generating the validation results.
    ///
    /// Additionally, all telemetry events received for the component for the validation run are
    /// provided as well.
    fn check_validation(
        &self,
        component_type: ComponentType,
        expectation: TestCaseExpectation,
        inputs: &[TestEvent],
        outputs: &[Event],
        telemetry_events: &[Event],
        runner_metrics: &RunnerMetrics,
    ) -> Result<Vec<String>, Vec<String>>;
}

/// Standard component validators.
///
/// This is an helper enum whose variants can trivially converted into a boxed `dyn Validator`
/// implementation, suitable for use with `Runner::add_validator`.
pub enum StandardValidators {
    /// Validates that the component meets the requirements of the [Component Specification][component_spec].
    ///
    /// See [`ComponentSpecValidator`] for more information.
    ///
    /// [component_spec]: https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md
    ComponentSpec,
}

impl From<StandardValidators> for Box<dyn Validator> {
    fn from(sv: StandardValidators) -> Self {
        match sv {
            StandardValidators::ComponentSpec => Box::<ComponentSpecValidator>::default(),
        }
    }
}

#[derive(PartialEq)]
pub enum ComponentMetricType {
    EventsReceived,
    EventsReceivedBytes,
    ReceivedBytesTotal,
    SentEventsTotal,
    SentBytesTotal,
    SentEventBytesTotal,
    ErrorsTotal,
    DiscardedEventsTotal,
}

impl ComponentMetricType {
    const fn name(&self) -> &'static str {
        match self {
            ComponentMetricType::EventsReceived => "component_received_events_total",
            ComponentMetricType::EventsReceivedBytes => "component_received_event_bytes_total",
            ComponentMetricType::ReceivedBytesTotal => "component_received_bytes_total",
            ComponentMetricType::SentEventsTotal => "component_sent_events_total",
            ComponentMetricType::SentBytesTotal => "component_sent_bytes_total",
            ComponentMetricType::SentEventBytesTotal => "component_sent_event_bytes_total",
            ComponentMetricType::ErrorsTotal => "component_errors_total",
            ComponentMetricType::DiscardedEventsTotal => "component_discarded_events_total",
        }
    }
}

impl Display for ComponentMetricType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
