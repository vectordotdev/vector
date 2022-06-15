use serde::{Deserialize, Serialize};

use crate::{
    config::component::ComponentDescription,
    event::{Event, EventArray, EventContainer, LogEvent, Metric, TraceEvent},
};

mod check_fields;
pub(self) mod datadog_search;
pub(crate) mod is_log;
pub(crate) mod is_metric;
pub mod not;
mod vrl;

pub use self::vrl::VrlConfig;

#[derive(Debug, Clone)]
pub enum Condition {
    Not(not::Not),
    IsLog(is_log::IsLog),
    IsMetric(is_metric::IsMetric),
    Vrl(vrl::Vrl),
    CheckFields(check_fields::CheckFields),
    DatadogSearch(datadog_search::DatadogSearchRunner),

    // used for benchmarks
    AlwaysPass,
    AlwaysFail,
}

impl Condition {
    pub(crate) const fn is_log() -> Self {
        Self::IsLog(is_log::IsLog {})
    }

    pub(crate) const fn is_metric() -> Self {
        Self::IsMetric(is_metric::IsMetric {})
    }
}

impl Condition {
    pub(crate) fn check(&self, event: Event) -> (bool, Event) {
        match event {
            Event::Log(log) => {
                let (result, log) = self.check_log(log);
                (result, Event::from(log))
            }
            Event::Metric(metric) => {
                let (result, metric) = self.check_metric(metric);
                (result, Event::from(metric))
            }
            Event::Trace(trace) => {
                let (result, trace) = self.check_trace(trace);
                (result, Event::from(trace))
            }
        }
    }

    pub(crate) fn check_log(&self, log: LogEvent) -> (bool, LogEvent) {
        match self {
            Condition::IsLog(x) => x.check_log(log),
            Condition::IsMetric(x) => x.check_log(log),
            Condition::Not(x) => x.check_log(log),
            Condition::CheckFields(x) => x.check_log(log),
            Condition::DatadogSearch(x) => x.check_log(log),
            Condition::Vrl(x) => x.check_log(log),
            Condition::AlwaysPass => (true, log),
            Condition::AlwaysFail => (false, log),
        }
    }

    pub(crate) fn check_metric(&self, metric: Metric) -> (bool, Metric) {
        match self {
            Condition::IsLog(x) => x.check_metric(metric),
            Condition::IsMetric(x) => x.check_metric(metric),
            Condition::Not(x) => x.check_metric(metric),
            Condition::CheckFields(x) => x.check_metric(metric),
            Condition::DatadogSearch(x) => x.check_metric(metric),
            Condition::Vrl(x) => x.check_metric(metric),
            Condition::AlwaysPass => (true, metric),
            Condition::AlwaysFail => (false, metric),
        }
    }

    pub(crate) fn check_trace(&self, trace: TraceEvent) -> (bool, TraceEvent) {
        match self {
            Condition::IsLog(x) => x.check_trace(trace),
            Condition::IsMetric(x) => x.check_trace(trace),
            Condition::Not(x) => x.check_trace(trace),
            Condition::CheckFields(x) => x.check_trace(trace),
            Condition::DatadogSearch(x) => x.check_trace(trace),
            Condition::Vrl(x) => x.check_trace(trace),
            Condition::AlwaysPass => (true, trace),
            Condition::AlwaysFail => (false, trace),
        }
    }

    pub(crate) fn check_all(&self, events: EventArray) -> Vec<(bool, Event)> {
        match self {
            Condition::IsLog(x) => x.check_all(events),
            Condition::IsMetric(x) => x.check_all(events),
            Condition::Not(x) => x.check_all(events),
            Condition::CheckFields(x) => x.check_all(events),
            Condition::DatadogSearch(x) => x.check_all(events),
            Condition::Vrl(x) => x.check_all(events),
            Condition::AlwaysPass => events.into_events().map(|event| (true, event)).collect(),
            Condition::AlwaysFail => events.into_events().map(|event| (false, event)).collect(),
        }
    }

    /// Provides context for a failure. This is potentially mildly expensive if
    /// it involves string building and so should be avoided in hot paths.
    pub(crate) fn check_with_context(&self, event: Event) -> (Result<(), String>, Event) {
        match self {
            Condition::IsLog(x) => x.check_with_context(event),
            Condition::IsMetric(x) => x.check_with_context(event),
            Condition::Not(x) => x.check_with_context(event),
            Condition::CheckFields(x) => x.check_with_context(event),
            Condition::DatadogSearch(x) => x.check_with_context(event),
            Condition::Vrl(x) => x.check_with_context(event),
            Condition::AlwaysPass => (Ok(()), event),
            Condition::AlwaysFail => (Ok(()), event),
        }
    }
}

pub trait Conditional {
    /// Checks if a condition is true. The event should not be modified, it is only mutable so it
    /// can be passed into VRL, but VRL type checking prevents mutation.
    fn check(&self, event: Event) -> (bool, Event) {
        match event {
            Event::Log(log) => {
                let (result, log) = self.check_log(log);
                (result, Event::from(log))
            }
            Event::Metric(metric) => {
                let (result, metric) = self.check_metric(metric);
                (result, Event::from(metric))
            }
            Event::Trace(trace) => {
                let (result, trace) = self.check_trace(trace);
                (result, Event::from(trace))
            }
        }
    }

    /// Checks if a condition is true. The log should not be modified, it is only mutable so it can
    /// be passed into VRL, but VRL type checking prevents mutation.
    fn check_log(&self, log: LogEvent) -> (bool, LogEvent);

    /// Checks if a condition is true. The metric should not be modified, it is only mutable so it
    /// can be passed into VRL, but VRL type checking prevents mutation.
    fn check_metric(&self, metric: Metric) -> (bool, Metric);

    /// Checks if a condition is true. The trace should not be modified, it is only mutable so it
    /// can be passed into VRL, but VRL type checking prevents mutation.
    fn check_trace(&self, trace: TraceEvent) -> (bool, TraceEvent);

    fn check_all(&self, events: EventArray) -> Vec<(bool, Event)> {
        match events {
            EventArray::Logs(logs) => logs
                .into_iter()
                .map(|log| {
                    let (result, log) = self.check_log(log);
                    (result, Event::from(log))
                })
                .collect::<Vec<_>>(),
            EventArray::Metrics(metrics) => metrics
                .into_iter()
                .map(|metric| {
                    let (result, metric) = self.check_metric(metric);
                    (result, Event::from(metric))
                })
                .collect::<Vec<_>>(),
            EventArray::Traces(traces) => traces
                .into_iter()
                .map(|trace| {
                    let (result, trace) = self.check_trace(trace);
                    (result, Event::from(trace))
                })
                .collect::<Vec<_>>(),
        }
    }

    /// Provides context for a failure. This is potentially mildly expensive if it involves string
    /// building and so should be avoided in hot paths.
    fn check_with_context(&self, e: Event) -> (Result<(), String>, Event) {
        let (result, event) = self.check(e);
        if result {
            (Ok(()), event)
        } else {
            (Err("condition failed".into()), event)
        }
    }
}

#[typetag::serde(tag = "type")]
pub trait ConditionConfig: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition>;
}

dyn_clone::clone_trait_object!(ConditionConfig);

type ConditionDescription = ComponentDescription<Box<dyn ConditionConfig>>;

inventory::collect!(ConditionDescription);

/// A condition can either be a raw string such as
/// `condition = '.message == "hooray"'`.
/// In this case it is turned into a VRL condition.
/// Otherwise it is a condition such as:
///
/// condition.type = 'check_fields'
/// condition."message.equals" = 'hooray'
///
///
/// It is important to note that because the way this is
/// structured, it is wrong to flatten a field that contains
/// an AnyCondition:
///
/// #[serde(flatten)]
/// condition: AnyCondition,
///
/// This will result in an error when serializing to json
/// which we need to do when determining which transforms have changed
/// when a config is reloaded.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum AnyCondition {
    String(String),
    Map(Box<dyn ConditionConfig>),
}

impl AnyCondition {
    pub fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition> {
        match self {
            AnyCondition::String(s) => VrlConfig {
                source: s.clone(),
                runtime: Default::default(),
            }
            .build(enrichment_tables),
            AnyCondition::Map(m) => m.build(enrichment_tables),
        }
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[derive(Deserialize, Debug)]
    struct Test {
        condition: AnyCondition,
    }

    #[test]
    fn deserialize_anycondition_default() {
        let conf: Test = toml::from_str(r#"condition = ".nork == false""#).unwrap();
        assert_eq!(
            r#"String(".nork == false")"#,
            format!("{:?}", conf.condition)
        )
    }

    #[test]
    fn deserialize_anycondition_check_fields() {
        let conf: Test = toml::from_str(indoc! {r#"
            condition.type = "check_fields"
            condition."norg.equals" = "nork"
        "#})
        .unwrap();

        assert_eq!(
            r#"Map(CheckFieldsConfig { predicates: {"norg.equals": "nork"} })"#,
            format!("{:?}", conf.condition)
        )
    }

    #[test]
    fn deserialize_anycondition_vrl() {
        let conf: Test = toml::from_str(indoc! {r#"
            condition.type = "vrl"
            condition.source = '.nork == true'
        "#})
        .unwrap();

        assert_eq!(
            r#"Map(VrlConfig { source: ".nork == true", runtime: Ast })"#,
            format!("{:?}", conf.condition)
        )
    }
}
