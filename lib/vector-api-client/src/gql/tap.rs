#![allow(clippy::upper_case_acronyms)]

use graphql_client::GraphQLQuery;

use crate::BoxedSubscription;

/// Shorthand for a Chrono datetime, set to UTC.
type DateTime = chrono::DateTime<chrono::Utc>;

/// OutputEventsByComponentIdPatternsSubscription allows observability into the events that are
/// generated from component(s).
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/output_events_by_component_id_patterns.graphql",
    response_derives = "Debug"
)]
pub struct OutputEventsByComponentIdPatternsSubscription;

/// Tap encoding format type that is more convenient to use for public clients than the
/// generated `output_events_by_component_id_patterns_subscription::EventEncodingType`.
#[derive(clap::ValueEnum, Debug, Clone, Copy)]
pub enum TapEncodingFormat {
    Json,
    Yaml,
    Logfmt,
}

/// String -> TapEncodingFormat, typically for parsing user input.
impl std::str::FromStr for TapEncodingFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "json" => Ok(Self::Json),
            "yaml" => Ok(Self::Yaml),
            "logfmt" => Ok(Self::Logfmt),
            _ => Err("Invalid encoding format".to_string()),
        }
    }
}

/// Map the public-facing `TapEncodingFormat` to the internal `EventEncodingType`.
impl From<TapEncodingFormat>
    for output_events_by_component_id_patterns_subscription::EventEncodingType
{
    fn from(encoding: TapEncodingFormat) -> Self {
        match encoding {
            TapEncodingFormat::Json => Self::JSON,
            TapEncodingFormat::Yaml => Self::YAML,
            TapEncodingFormat::Logfmt => Self::LOGFMT,
        }
    }
}

pub trait TapSubscriptionExt {
    /// Executes an output events subscription.
    fn output_events_by_component_id_patterns_subscription(
        &self,
        outputs_patterns: Vec<String>,
        inputs_patterns: Vec<String>,
        encoding: TapEncodingFormat,
        limit: i64,
        interval: i64,
    ) -> crate::BoxedSubscription<OutputEventsByComponentIdPatternsSubscription>;
}

impl TapSubscriptionExt for crate::SubscriptionClient {
    /// Executes an output events subscription.
    fn output_events_by_component_id_patterns_subscription(
        &self,
        outputs_patterns: Vec<String>,
        inputs_patterns: Vec<String>,
        encoding: TapEncodingFormat,
        limit: i64,
        interval: i64,
    ) -> BoxedSubscription<OutputEventsByComponentIdPatternsSubscription> {
        let request_body = OutputEventsByComponentIdPatternsSubscription::build_query(
            output_events_by_component_id_patterns_subscription::Variables {
                outputs_patterns,
                inputs_patterns: Some(inputs_patterns),
                limit,
                interval,
                encoding: encoding.into(),
            },
        );

        self.start::<OutputEventsByComponentIdPatternsSubscription>(&request_body)
    }
}
