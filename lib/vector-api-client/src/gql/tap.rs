use crate::BoxedSubscription;
use graphql_client::GraphQLQuery;

/// Shorthand for a Chrono datetime, set to UTC.
type DateTime = chrono::DateTime<chrono::Utc>;

/// OutputLogEventsSubscription allows observability into the log events that are
/// generated from component(s).
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/output_log_events.graphql",
    response_derives = "Debug"
)]
pub struct OutputLogEventsSubscription;

pub trait TapSubscriptionExt {
    /// Executes an output log events subscription.
    fn output_log_events_subscription(
        &self,
        component_names: Vec<String>,
        encoding: output_log_events_subscription::LogEventEncodingType,
        limit: i64,
        interval: i64,
    ) -> crate::BoxedSubscription<OutputLogEventsSubscription>;
}

impl TapSubscriptionExt for crate::SubscriptionClient {
    /// Executes an output log events subscription.
    fn output_log_events_subscription(
        &self,
        component_names: Vec<String>,
        encoding: output_log_events_subscription::LogEventEncodingType,
        limit: i64,
        interval: i64,
    ) -> BoxedSubscription<OutputLogEventsSubscription> {
        let request_body =
            OutputLogEventsSubscription::build_query(output_log_events_subscription::Variables {
                component_names,
                limit,
                interval,
                encoding,
            });

        self.start::<OutputLogEventsSubscription>(&request_body)
    }
}
