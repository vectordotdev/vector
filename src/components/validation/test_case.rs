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
    pub expectation: TestCaseExpectation,
    pub events: Vec<TestEvent>,
}
