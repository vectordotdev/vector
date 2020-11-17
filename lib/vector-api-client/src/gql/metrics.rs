//! Metrics queries/subscriptions

use crate::BoxedSubscription;
use graphql_client::GraphQLQuery;

/// UptimeSubscription returns uptime metrics to determine how long the Vector
/// instance has been running
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/uptime.graphql",
    response_derives = "Debug"
)]
pub struct UptimeSubscription;

/// EventsProcessedTotalSubscription contains metrics on the number of events
/// that have been processed by a Vector instance
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/events_processed_total.graphql",
    response_derives = "Debug"
)]
pub struct EventsProcessedTotalSubscription;

/// EventsProcessedThroughputSubscription contains metrics on the number of events
/// that have been processed between `interval` samples
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/events_processed_throughput.graphql",
    response_derives = "Debug"
)]
pub struct EventsProcessedThroughputSubscription;

/// BytesProcessedThroughputSubscription contains metrics on the number of bytes
/// that have been processed between `interval` samples
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/bytes_processed_throughput.graphql",
    response_derives = "Debug"
)]
pub struct BytesProcessedThroughputSubscription;

/// ComponentEventsProcessedThroughputsSubscription contains metrics on the number of events
/// that have been processed between `interval` samples, against specific components
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_events_processed_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct ComponentEventsProcessedThroughputsSubscription;

/// ComponentEventsProcessedTotalsSubscription contains metrics on the number of events
/// that have been processed by a Vector instance, against specific components
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_events_processed_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentEventsProcessedTotalsSubscription;

/// ComponentBytesProcessedThroughputsSubscription contains metrics on the number of bytes
/// that have been processed between `interval` samples, against specific components
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_bytes_processed_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct ComponentBytesProcessedThroughputsSubscription;

/// ComponentBytesProcessedTotalsSubscription contains metrics on the number of bytes
/// that have been processed by a Vector instance, against a specific component
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_bytes_processed_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentBytesProcessedTotalsSubscription;

/// Extension methods for metrics subscriptions
pub trait MetricsSubscriptionExt {
    /// Executes an uptime metrics subscription
    fn uptime_subscription(&self) -> crate::BoxedSubscription<UptimeSubscription>;

    /// Executes an events processed metrics subscription
    fn events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<EventsProcessedTotalSubscription>;

    /// Executes an events processed throughput subscription
    fn events_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<EventsProcessedThroughputSubscription>;

    /// Executes a bytes processed throughput subscription
    fn bytes_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<BytesProcessedThroughputSubscription>;

    /// Executes an component events processed totals subscription
    fn component_events_processed_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsProcessedTotalsSubscription>;

    /// Executes an component events processed throughputs subscription
    fn component_events_processed_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsProcessedThroughputsSubscription>;

    /// Executes an component bytes processed totals subscription
    fn component_bytes_processed_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentBytesProcessedTotalsSubscription>;

    /// Executes an component bytes processed throughputs subscription
    fn component_bytes_processed_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentBytesProcessedThroughputsSubscription>;
}

impl MetricsSubscriptionExt for crate::SubscriptionClient {
    /// Executes an uptime metrics subscription
    fn uptime_subscription(&self) -> BoxedSubscription<UptimeSubscription> {
        let request_body = UptimeSubscription::build_query(uptime_subscription::Variables);

        self.start::<UptimeSubscription>(&request_body)
    }

    /// Executes an events processed metrics subscription
    fn events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<EventsProcessedTotalSubscription> {
        let request_body = EventsProcessedTotalSubscription::build_query(
            events_processed_total_subscription::Variables { interval },
        );

        self.start::<EventsProcessedTotalSubscription>(&request_body)
    }

    /// Executes an events processed throughput subscription
    fn events_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<EventsProcessedThroughputSubscription> {
        let request_body = EventsProcessedThroughputSubscription::build_query(
            events_processed_throughput_subscription::Variables { interval },
        );

        self.start::<EventsProcessedThroughputSubscription>(&request_body)
    }

    /// Executes a bytes processed throughput subscription
    fn bytes_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<BytesProcessedThroughputSubscription> {
        let request_body = BytesProcessedThroughputSubscription::build_query(
            bytes_processed_throughput_subscription::Variables { interval },
        );

        self.start::<BytesProcessedThroughputSubscription>(&request_body)
    }

    /// Executes an all component events processed totals subscription
    fn component_events_processed_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentEventsProcessedTotalsSubscription> {
        let request_body = ComponentEventsProcessedTotalsSubscription::build_query(
            component_events_processed_totals_subscription::Variables { interval },
        );

        self.start::<ComponentEventsProcessedTotalsSubscription>(&request_body)
    }

    /// Executes an all component events processed throughputs subscription
    fn component_events_processed_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentEventsProcessedThroughputsSubscription> {
        let request_body = ComponentEventsProcessedThroughputsSubscription::build_query(
            component_events_processed_throughputs_subscription::Variables { interval },
        );

        self.start::<ComponentEventsProcessedThroughputsSubscription>(&request_body)
    }

    /// Executes an all component bytes processed totals subscription
    fn component_bytes_processed_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentBytesProcessedTotalsSubscription> {
        let request_body = ComponentBytesProcessedTotalsSubscription::build_query(
            component_bytes_processed_totals_subscription::Variables { interval },
        );

        self.start::<ComponentBytesProcessedTotalsSubscription>(&request_body)
    }

    /// Executes an all component bytes processed throughputs subscription
    fn component_bytes_processed_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentBytesProcessedThroughputsSubscription> {
        let request_body = ComponentBytesProcessedThroughputsSubscription::build_query(
            component_bytes_processed_throughputs_subscription::Variables { interval },
        );

        self.start::<ComponentBytesProcessedThroughputsSubscription>(&request_body)
    }
}
