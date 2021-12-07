mod filter;
pub mod test_util;

pub use filter::EventFilter;

#[cfg(test)]
mod tests {
    use super::*;

    use datadog_filter::build_matcher;
    use datadog_search_syntax::parse;

    #[test]
    /// Parse each Datadog Search Syntax query and check that it passes/fails.
    fn checks() {
        let checks = test_util::get_checks();
        let filter = EventFilter::default();

        for (source, pass, fail) in checks {
            let node = parse(source).unwrap();
            let matcher = build_matcher(&node, &filter);

            assert!(matcher.run(pass.as_log()));
            assert!(!matcher.run(&fail.as_log()));
        }
    }
}
