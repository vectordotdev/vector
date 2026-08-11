use serde::Deserialize;

use super::TestEvent;

/// Expected outcome of a validation test case.
#[derive(Clone, Copy, Deserialize)]
pub enum TestCaseExpectation {
    /// All events were processed successfully.
    #[serde(rename = "success")]
    Success,

    /// All events failed to be processed successfully.
    #[serde(rename = "failure")]
    Failure,

    /// Some events, but not all, were processed successfully.
    #[serde(rename = "partial_success")]
    PartialSuccess,
}

/// A validation test case.
///
/// Test cases define both the events that should be given as input to the component being
/// validated, as well as the "expectation" for the test case, in terms of if all the events should
/// be processed successfully, or fail to be processed, and so on.
#[derive(Deserialize)]
pub struct TestCase {
    pub name: String,
    pub config_name: Option<String>,
    pub expectation: TestCaseExpectation,
    pub events: Vec<TestEvent>,

    /// How many `component_errors_total` increments the component records for each failing event.
    ///
    /// Defaults to one. Some components record several distinct errors for a single failure, such
    /// as a decoding error followed by the rejected request it causes.
    #[serde(default = "default_errors_per_failure")]
    pub errors_per_failure: u64,
}

const fn default_errors_per_failure() -> u64 {
    1
}
