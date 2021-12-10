mod filter;

pub use filter::EventFilter;

#[cfg(test)]
mod tests {
    use super::*;
    use datadog_filter_test::test_filter;

    #[test]
    /// Parse each Datadog Search Syntax query and check that it passes/fails.
    fn event_filter() {
        test_filter(EventFilter::default(), |ev| ev.into_log())
    }
}
