//! Metrics queries/subscriptions.

use graphql_client::GraphQLQuery;

use crate::BoxedSubscription;

/// UptimeSubscription returns uptime metrics to determine how long the Vector
/// instance has been running.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/uptime.graphql",
    response_derives = "Debug"
)]
pub struct UptimeSubscription;

/// ProcessedEventsTotalSubscription contains metrics on the number of events
/// that have been processed by a Vector instance.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/processed_events_total.graphql",
    response_derives = "Debug"
)]
pub struct ProcessedEventsTotalSubscription;

/// ProcessedEventsThroughputSubscription contains metrics on the number of events
/// that have been processed between `interval` samples.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/processed_events_throughput.graphql",
    response_derives = "Debug"
)]
pub struct ProcessedEventsThroughputSubscription;

/// ProcessedBytesThroughputSubscription contains metrics on the number of bytes
/// that have been processed between `interval` samples.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/processed_bytes_throughput.graphql",
    response_derives = "Debug"
)]
pub struct ProcessedBytesThroughputSubscription;

/// ComponentProcessedEventsThroughputsSubscription contains metrics on the number of events
/// that have been processed between `interval` samples, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_processed_events_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct ComponentProcessedEventsThroughputsSubscription;

/// ComponentProcessedEventsTotalsSubscription contains metrics on the number of events
/// that have been processed by a Vector instance, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_processed_events_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentProcessedEventsTotalsSubscription;

/// ComponentAllocatedBytesSubscription contains metrics on the number of allocated bytes
/// that have been processed by a Vector instance, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_allocated_bytes.graphql",
    response_derives = "Debug"
)]
pub struct ComponentAllocatedBytesSubscription;

/// ComponentProcessedBytesThroughputsSubscription contains metrics on the number of bytes
/// that have been processed between `interval` samples, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_processed_bytes_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct ComponentProcessedBytesThroughputsSubscription;

/// ComponentProcessedBytesTotalsSubscription contains metrics on the number of bytes
/// that have been processed by a Vector instance, against a specific component.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_processed_bytes_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentProcessedBytesTotalsSubscription;

/// ComponentReceivedEventsThroughputsSubscription contains metrics on the number of events
/// that have been accepted for processing between `interval` samples, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_received_events_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct ComponentReceivedEventsThroughputsSubscription;

/// ComponentReceivedEventsTotalsSubscription contains metrics on the number of events
/// that have been accepted for processing by a Vector instance, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_received_events_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentReceivedEventsTotalsSubscription;

/// ComponentSentEventsThroughputsSubscription contains metrics on the number of events
/// that have been emitted between `interval` samples, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_sent_events_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct ComponentSentEventsThroughputsSubscription;

/// ComponentSentEventsTotalsSubscription contains metrics on the number of events
/// that have been emitted by a Vector instance, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_sent_events_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentSentEventsTotalsSubscription;

impl component_sent_events_totals_subscription::ComponentSentEventsTotalsSubscriptionComponentSentEventsTotals {
    pub fn outputs(&self) -> Vec<(String, i64)> {
        self.outputs
            .iter()
            .map(|output| {
                (
                    output.output_id.clone(),
                    output
                        .sent_events_total
                        .as_ref()
                        .map(|p| p.sent_events_total as i64)
                        .unwrap_or(0),
                )
            })
            .collect()
    }
}

impl component_sent_events_throughputs_subscription::ComponentSentEventsThroughputsSubscriptionComponentSentEventsThroughputs {
    pub fn outputs(&self) -> Vec<(String, i64)> {
        self.outputs
            .iter()
            .map(|output| {
                (
                    output.output_id.clone(),
                    output.throughput,
                )
            })
            .collect()
    }

}

/// ComponentErrorsTotalsSubscription contains metrics on the number of errors
/// (metrics ending in `_errors_total`), against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_errors_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentErrorsTotalsSubscription;

/// Extension methods for metrics subscriptions
pub trait MetricsSubscriptionExt {
    /// Executes an uptime metrics subscription.
    fn uptime_subscription(&self) -> crate::BoxedSubscription<UptimeSubscription>;

    /// Executes an all component allocated bytes subscription.
    fn component_allocated_bytes_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentAllocatedBytesSubscription>;

    /// Executes an events processed metrics subscription.
    fn processed_events_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ProcessedEventsTotalSubscription>;

    /// Executes an events processed throughput subscription.
    fn processed_events_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ProcessedEventsThroughputSubscription>;

    /// Executes a bytes processed throughput subscription.
    fn processed_bytes_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ProcessedBytesThroughputSubscription>;

    /// Executes a component events processed totals subscription
    fn component_processed_events_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentProcessedEventsTotalsSubscription>;

    /// Executes a component events processed throughputs subscription.
    fn component_processed_events_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentProcessedEventsThroughputsSubscription>;

    /// Executes a component bytes processed totals subscription.
    fn component_processed_bytes_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentProcessedBytesTotalsSubscription>;

    /// Executes a component bytes processed throughputs subscription.
    fn component_processed_bytes_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentProcessedBytesThroughputsSubscription>;

    /// Executes a component received events totals subscription.
    fn component_received_events_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentReceivedEventsTotalsSubscription>;

    /// Executes an component events in throughputs subscription.
    fn component_received_events_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentReceivedEventsThroughputsSubscription>;

    /// Executes a component events totals subscription.
    fn component_sent_events_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentSentEventsTotalsSubscription>;

    /// Executes a component sent events throughputs subscription.
    fn component_sent_events_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentSentEventsThroughputsSubscription>;

    fn component_errors_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentErrorsTotalsSubscription>;
}

impl MetricsSubscriptionExt for crate::SubscriptionClient {
    /// Executes an uptime metrics subscription.
    fn uptime_subscription(&self) -> BoxedSubscription<UptimeSubscription> {
        let request_body = UptimeSubscription::build_query(uptime_subscription::Variables);

        self.start::<UptimeSubscription>(&request_body)
    }

    /// Executes an events processed metrics subscription.
    fn processed_events_total_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ProcessedEventsTotalSubscription> {
        let request_body = ProcessedEventsTotalSubscription::build_query(
            processed_events_total_subscription::Variables { interval },
        );

        self.start::<ProcessedEventsTotalSubscription>(&request_body)
    }

    /// Executes an events processed throughput subscription.
    fn processed_events_throughput_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ProcessedEventsThroughputSubscription> {
        let request_body = ProcessedEventsThroughputSubscription::build_query(
            processed_events_throughput_subscription::Variables { interval },
        );

        self.start::<ProcessedEventsThroughputSubscription>(&request_body)
    }

    /// Executes a bytes processed throughput subscription.
    fn processed_bytes_throughput_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ProcessedBytesThroughputSubscription> {
        let request_body = ProcessedBytesThroughputSubscription::build_query(
            processed_bytes_throughput_subscription::Variables { interval },
        );

        self.start::<ProcessedBytesThroughputSubscription>(&request_body)
    }

    /// Executes an all component events processed totals subscription.
    fn component_processed_events_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentProcessedEventsTotalsSubscription> {
        let request_body = ComponentProcessedEventsTotalsSubscription::build_query(
            component_processed_events_totals_subscription::Variables { interval },
        );

        self.start::<ComponentProcessedEventsTotalsSubscription>(&request_body)
    }

    /// Executes an all component events processed throughputs subscription.
    fn component_processed_events_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentProcessedEventsThroughputsSubscription> {
        let request_body = ComponentProcessedEventsThroughputsSubscription::build_query(
            component_processed_events_throughputs_subscription::Variables { interval },
        );

        self.start::<ComponentProcessedEventsThroughputsSubscription>(&request_body)
    }

    /// Executes an all component bytes processed totals subscription.
    fn component_processed_bytes_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentProcessedBytesTotalsSubscription> {
        let request_body = ComponentProcessedBytesTotalsSubscription::build_query(
            component_processed_bytes_totals_subscription::Variables { interval },
        );

        self.start::<ComponentProcessedBytesTotalsSubscription>(&request_body)
    }

    /// Executes an all component bytes processed throughputs subscription.
    fn component_processed_bytes_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentProcessedBytesThroughputsSubscription> {
        let request_body = ComponentProcessedBytesThroughputsSubscription::build_query(
            component_processed_bytes_throughputs_subscription::Variables { interval },
        );

        self.start::<ComponentProcessedBytesThroughputsSubscription>(&request_body)
    }

    /// Executes an all component allocated bytes subscription.
    fn component_allocated_bytes_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentAllocatedBytesSubscription> {
        let request_body = ComponentAllocatedBytesSubscription::build_query(
            component_allocated_bytes_subscription::Variables { interval },
        );

        self.start::<ComponentAllocatedBytesSubscription>(&request_body)
    }
    /// Executes an all component received events totals subscription.
    fn component_received_events_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentReceivedEventsTotalsSubscription> {
        let request_body = ComponentReceivedEventsTotalsSubscription::build_query(
            component_received_events_totals_subscription::Variables { interval },
        );

        self.start::<ComponentReceivedEventsTotalsSubscription>(&request_body)
    }

    /// Executes an all component received events throughputs subscription.
    fn component_received_events_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentReceivedEventsThroughputsSubscription> {
        let request_body = ComponentReceivedEventsThroughputsSubscription::build_query(
            component_received_events_throughputs_subscription::Variables { interval },
        );

        self.start::<ComponentReceivedEventsThroughputsSubscription>(&request_body)
    }

    /// Executes a component sent events totals subscription.
    fn component_sent_events_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentSentEventsTotalsSubscription> {
        let request_body = ComponentSentEventsTotalsSubscription::build_query(
            component_sent_events_totals_subscription::Variables { interval },
        );

        self.start::<ComponentSentEventsTotalsSubscription>(&request_body)
    }

    /// Executes a component sent events throughputs subscription.
    fn component_sent_events_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentSentEventsThroughputsSubscription> {
        let request_body = ComponentSentEventsThroughputsSubscription::build_query(
            component_sent_events_throughputs_subscription::Variables { interval },
        );

        self.start::<ComponentSentEventsThroughputsSubscription>(&request_body)
    }

    fn component_errors_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentErrorsTotalsSubscription> {
        let request_body = ComponentErrorsTotalsSubscription::build_query(
            component_errors_totals_subscription::Variables { interval },
        );

        self.start::<ComponentErrorsTotalsSubscription>(&request_body)
    }
}
