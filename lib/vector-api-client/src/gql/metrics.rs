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

/// ComponentAllocatedBytesSubscription contains metrics on the number of allocated bytes
/// that have been processed by a Vector instance, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_allocated_bytes.graphql",
    response_derives = "Debug"
)]
pub struct ComponentAllocatedBytesSubscription;

/// ComponentReceivedBytesThroughputsSubscription contains metrics on the number of bytes
/// that have been received between `interval` samples, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_received_bytes_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct ComponentReceivedBytesThroughputsSubscription;

/// ComponentReceivedBytesTotalsSubscription contains metrics on the number of bytes
/// that have been received by a Vector instance, against a specific component.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_received_bytes_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentReceivedBytesTotalsSubscription;

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

/// ComponentSentBytesThroughputsSubscription contains metrics on the number of bytes
/// that have been received between `interval` samples, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_sent_bytes_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct ComponentSentBytesThroughputsSubscription;

/// ComponentSentBytesTotalsSubscription contains metrics on the number of bytes
/// that have been received by a Vector instance, against a specific component.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_sent_bytes_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentSentBytesTotalsSubscription;

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

    /// Executes a component bytes received totals subscription.
    fn component_received_bytes_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentReceivedBytesTotalsSubscription>;

    /// Executes a component bytes received throughput subscription.
    fn component_received_bytes_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentReceivedBytesThroughputsSubscription>;

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

    /// Executes a component bytes sent totals subscription.
    fn component_sent_bytes_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentSentBytesTotalsSubscription>;

    /// Executes a component bytes sent throughput subscription.
    fn component_sent_bytes_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentSentBytesThroughputsSubscription>;

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

    /// Executes an all component bytes received totals subscription.
    fn component_received_bytes_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentReceivedBytesTotalsSubscription> {
        let request_body = ComponentReceivedBytesTotalsSubscription::build_query(
            component_received_bytes_totals_subscription::Variables { interval },
        );

        self.start::<ComponentReceivedBytesTotalsSubscription>(&request_body)
    }

    /// Executes a component bytes received throughput subscription.
    fn component_received_bytes_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentReceivedBytesThroughputsSubscription> {
        let request_body = ComponentReceivedBytesThroughputsSubscription::build_query(
            component_received_bytes_throughputs_subscription::Variables { interval },
        );

        self.start::<ComponentReceivedBytesThroughputsSubscription>(&request_body)
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

    /// Executes an all component bytes sent totals subscription.
    fn component_sent_bytes_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentSentBytesTotalsSubscription> {
        let request_body = ComponentSentBytesTotalsSubscription::build_query(
            component_sent_bytes_totals_subscription::Variables { interval },
        );

        self.start::<ComponentSentBytesTotalsSubscription>(&request_body)
    }

    /// Executes a component bytes sent throughput subscription.
    fn component_sent_bytes_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentSentBytesThroughputsSubscription> {
        let request_body = ComponentSentBytesThroughputsSubscription::build_query(
            component_sent_bytes_throughputs_subscription::Variables { interval },
        );

        self.start::<ComponentSentBytesThroughputsSubscription>(&request_body)
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
