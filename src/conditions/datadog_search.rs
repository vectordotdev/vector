use datadog_filter::{build_matcher, Matcher, Run};
use datadog_search_syntax::parse;
use serde::{Deserialize, Serialize};
use vector_datadog_filter::EventFilter;

use crate::{
    conditions::{Condition, ConditionConfig, ConditionDescription},
    event::{Event, LogEvent},
};

#[derive(Deserialize, Serialize, Debug, Default, Clone, PartialEq)]
pub struct DatadogSearchConfig {
    pub source: String,
}

inventory::submit! {
    ConditionDescription::new::<DatadogSearchConfig>("datadog_search")
}

impl_generate_config_from_default!(DatadogSearchConfig);

/// Runner that contains the boxed `Matcher` function to check whether an `Event` matches
/// a Datadog Search Syntax query.
#[derive(Clone)]
struct DatadogSearchRunner {
    matcher: Box<dyn Matcher<Event>>,
}

impl Condition for DatadogSearchRunner {
    fn check(&self, e: &Event) -> bool {
        self.matcher.run(e)
    }
}

#[typetag::serde(name = "datadog_search")]
impl ConditionConfig for DatadogSearchConfig {
    fn build(
        &self,
        _enrichment_tables: &enrichment::TableRegistry,
    ) -> crate::Result<Box<dyn Condition>> {
        let node = parse(&self.source)?;
        let matcher = as_log(build_matcher(&node, &EventFilter::default()));

        Ok(Box::new(DatadogSearchRunner { matcher }))
    }
}

/// Run the provided `Matcher` when we're dealing with `LogEvent`s. Otherwise, return false.
fn as_log(matcher: Box<dyn Matcher<LogEvent>>) -> Box<dyn Matcher<Event>> {
    Run::boxed(move |ev| match ev {
        Event::Log(log) => matcher.run(log),
        _ => false,
    })
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use datadog_filter_test::get_checks;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogSearchConfig>();
    }

    #[test]
    fn check_datadog() {
        for (source, pass, fail) in get_checks() {
            let config = DatadogSearchConfig {
                source: source.to_owned(),
            };

            // Every query should build successfully.
            let cond = config
                .build(&Default::default())
                .unwrap_or_else(|_| panic!("build failed: {}", source));

            assert!(
                cond.check_with_context(&pass).is_ok(),
                "should pass: {}\nevent: {:?}",
                source,
                pass.as_log()
            );

            assert!(
                cond.check_with_context(&fail).is_err(),
                "should fail: {}\nevent: {:?}",
                source,
                fail.as_log()
            );
        }
    }
}
