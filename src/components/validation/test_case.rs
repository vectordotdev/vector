use super::TestEvent;

/// Expected outcome of a validation test case.
#[derive(Clone, Copy)]
pub enum TestCaseExpectation {
    /// All events were processed successfully.
    Success,

    /// All events failed to be processed successfully.
    Failure,

    /// Some events failed to be processed successfully.
    PartialFailure,
}

/// A validation test case.
///
/// Test cases define both the events that should be given as input to the component being
/// validated, as well as the "expectation" for the test case, in terms of if all the events should
/// be processed successfully, or fail to be processed, and so on.
pub struct TestCase {
    pub expectation: TestCaseExpectation,
    pub events: Vec<TestEvent>,
}

impl TestCase {
    /// Creates a test case where all events should be processed successfully.
    pub fn success<I, E>(events: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: Into<TestEvent>,
    {
        Self::from_events(TestCaseExpectation::Success, events)
    }

    /// Creates a test case where all events should fail to be processed successfully.
    pub fn failure<I, E>(events: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: Into<TestEvent>,
    {
        Self::from_events(TestCaseExpectation::Failure, events)
    }

    /// Creates a test case where some events should fail to be processed successfully.
    pub fn partial_failure<I, E>(events: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: Into<TestEvent>,
    {
        Self::from_events(TestCaseExpectation::PartialFailure, events)
    }

    fn from_events<I, E>(expectation: TestCaseExpectation, events: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: Into<TestEvent>,
    {
        Self {
            expectation,
            events: events.into_iter().map(Into::into).collect(),
        }
    }
}
