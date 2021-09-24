#![allow(clippy::upper_case_acronyms)]

use crate::BoxedSubscription;
use graphql_client::GraphQLQuery;

/// Shorthand for a Chrono datetime, set to UTC.
type DateTime = chrono::DateTime<chrono::Utc>;

/// OutputEventsSubscription allows observability into the events that are
/// generated from component(s).
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/output_events.graphql",
    response_derives = "Debug"
)]
pub struct OutputEventsSubscription;

/// Tap encoding format type that is more convenient to use for public clients than the
/// generated `output_events_subscription::EventEncodingType`.
#[derive(Debug, Clone, Copy)]
pub enum TapEncodingFormat {
    Json,
    Yaml,
}

/// String -> TapEncodingFormat, typically for parsing user input.
impl std::str::FromStr for TapEncodingFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "json" => Ok(Self::Json),
            "yaml" => Ok(Self::Yaml),
            _ => Err("Invalid encoding format".to_string()),
        }
    }
}

/// Map the public-facing `TapEncodingFormat` to the internal `EventEncodingType`.
impl From<TapEncodingFormat> for output_events_subscription::EventEncodingType {
    fn from(encoding: TapEncodingFormat) -> Self {
        match encoding {
            TapEncodingFormat::Json => Self::JSON,
            TapEncodingFormat::Yaml => Self::YAML,
        }
    }
}

impl output_events_subscription::OutputEventsSubscriptionOutputEvents {
    pub fn as_log(
        &self,
    ) -> Option<&output_events_subscription::OutputEventsSubscriptionOutputEventsOnLog> {
        match self {
            output_events_subscription::OutputEventsSubscriptionOutputEvents::Log(ev) => Some(ev),
            _ => None,
        }
    }
}

pub trait TapSubscriptionExt {
    /// Executes an output events subscription.
    fn output_events_subscription(
        &self,
        component_patterns: Vec<String>,
        encoding: TapEncodingFormat,
        limit: i64,
        interval: i64,
    ) -> crate::BoxedSubscription<OutputEventsSubscription>;
}

impl TapSubscriptionExt for crate::SubscriptionClient {
    /// Executes an output events subscription.
    fn output_events_subscription(
        &self,
        component_patterns: Vec<String>,
        encoding: TapEncodingFormat,
        limit: i64,
        interval: i64,
    ) -> BoxedSubscription<OutputEventsSubscription> {
        let request_body =
            OutputEventsSubscription::build_query(output_events_subscription::Variables {
                component_patterns,
                limit,
                interval,
                encoding: encoding.into(),
            });

        self.start::<OutputEventsSubscription>(&request_body)
    }
}
