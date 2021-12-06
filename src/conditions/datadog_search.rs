use crate::conditions::{Condition, ConditionConfig, ConditionDescription};
use crate::event::Event;
use datadog_filter::{build_matcher, Matcher};
use datadog_search_syntax::parse;
use serde::{Deserialize, Serialize};
use vector_datadog_filter::EventFilter;

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
        let matcher = build_matcher(&node, &EventFilter::default());

        Ok(Box::new(DatadogSearchRunner { matcher }))
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use vector_datadog_filter::test_util::get_checks;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DatadogSearchConfig>();
    }

    #[test]
    fn check_datadog() {
        let checks = get_checks();

        for (source, pass, fail) in checks {
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
                pass
            );

            assert!(
                cond.check_with_context(&fail).is_err(),
                "should fail: {}\nevent: {:?}",
                source,
                fail
            );
        }
    }
}
