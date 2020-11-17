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

/// ComponentEventsProcessedTotalSubscription contains metrics on the number of events
/// that have been processed by a Vector instance, against a specific component
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_events_processed_total.graphql",
    response_derives = "Debug"
)]
pub struct ComponentEventsProcessedTotalSubscription;

/// ComponentEventsProcessedThroughputSubscription contains metrics on the number of events
/// that have been processed between `interval` samples, against a specific component
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_events_processed_throughput.graphql",
    response_derives = "Debug"
)]

pub struct ComponentEventsProcessedThroughputSubscription;

/// AllComponentEventsProcessedThroughputsSubscription contains metrics on the number of events
/// that have been processed between `interval` samples, against specific components
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/all_component_events_processed_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct AllComponentEventsProcessedThroughputsSubscription;

/// AllComponentEventsProcessedTotalsSubscription contains metrics on the number of events
/// that have been processed by a Vector instance, against specific components
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/all_component_events_processed_totals.graphql",
    response_derives = "Debug"
)]
pub struct AllComponentEventsProcessedTotalsSubscription;

/// ComponentBytesProcessedTotalSubscription contains metrics on the number of bytes
/// that have been processed by a Vector instance, against a specific component
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_bytes_processed_total.graphql",
    response_derives = "Debug"
)]
pub struct ComponentBytesProcessedTotalSubscription;

/// ComponentBytesProcessedThroughputSubscription contains metrics on the number of bytes
/// that have been processed between `interval` samples, against a specific component
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_bytes_processed_throughput.graphql",
    response_derives = "Debug"
)]
pub struct ComponentBytesProcessedThroughputSubscription;

/// AllComponentBytesProcessedThroughputsSubscription contains metrics on the number of bytes
/// that have been processed between `interval` samples, against specific components
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/all_component_bytes_processed_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct AllComponentBytesProcessedThroughputsSubscription;

/// AllComponentBytesProcessedTotalsSubscription contains metrics on the number of bytes
/// that have been processed by a Vector instance, against a specific component
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/all_component_bytes_processed_totals.graphql",
    response_derives = "Debug"
)]
pub struct AllComponentBytesProcessedTotalsSubscription;

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

    /// Executes a component events processed total metrics subscription
    fn component_events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsProcessedTotalSubscription>;

    /// Executes an all component events processed totals subscription
    fn all_component_events_processed_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<AllComponentEventsProcessedTotalsSubscription>;

    /// Executes a component events processed throughput metrics subscription
    fn component_events_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsProcessedThroughputSubscription>;

    /// Executes an all component events processed throughputs subscription
    fn all_component_events_processed_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<AllComponentEventsProcessedThroughputsSubscription>;

    /// Executes a component bytes processed total metrics subscription
    fn component_bytes_processed_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentBytesProcessedTotalSubscription>;

    /// Executes an all component bytes processed totals subscription
    fn all_component_bytes_processed_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<AllComponentBytesProcessedTotalsSubscription>;

    /// Executes a component bytes processed throughput metrics subscription
    fn component_bytes_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentBytesProcessedThroughputSubscription>;

    /// Executes an all component bytes processed throughputs subscription
    fn all_component_bytes_processed_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<AllComponentBytesProcessedThroughputsSubscription>;
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

    /// Executes a component events processed total metrics subscription
    fn component_events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentEventsProcessedTotalSubscription> {
        let request_body = ComponentEventsProcessedTotalSubscription::build_query(
            component_events_processed_total_subscription::Variables { interval },
        );

        self.start::<ComponentEventsProcessedTotalSubscription>(&request_body)
    }

    /// Executes an all component events processed totals subscription
    fn all_component_events_processed_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<AllComponentEventsProcessedTotalsSubscription> {
        let request_body = AllComponentEventsProcessedTotalsSubscription::build_query(
            all_component_events_processed_totals_subscription::Variables { interval },
        );

        self.start::<AllComponentEventsProcessedTotalsSubscription>(&request_body)
    }

    /// Executes a component events processed throughput subscription
    fn component_events_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentEventsProcessedThroughputSubscription> {
        let request_body = ComponentEventsProcessedThroughputSubscription::build_query(
            component_events_processed_throughput_subscription::Variables { interval },
        );

        self.start::<ComponentEventsProcessedThroughputSubscription>(&request_body)
    }

    /// Executes an all component events processed throughputs subscription
    fn all_component_events_processed_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<AllComponentEventsProcessedThroughputsSubscription> {
        let request_body = AllComponentEventsProcessedThroughputsSubscription::build_query(
            all_component_events_processed_throughputs_subscription::Variables { interval },
        );

        self.start::<AllComponentEventsProcessedThroughputsSubscription>(&request_body)
    }
    /// Executes a component bytes processed total metrics subscription
    fn component_bytes_processed_total_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentBytesProcessedTotalSubscription> {
        let request_body = ComponentBytesProcessedTotalSubscription::build_query(
            component_bytes_processed_total_subscription::Variables { interval },
        );

        self.start::<ComponentBytesProcessedTotalSubscription>(&request_body)
    }

    /// Executes an all component bytes processed totals subscription
    fn all_component_bytes_processed_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<AllComponentBytesProcessedTotalsSubscription> {
        let request_body = AllComponentBytesProcessedTotalsSubscription::build_query(
            all_component_bytes_processed_totals_subscription::Variables { interval },
        );

        self.start::<AllComponentBytesProcessedTotalsSubscription>(&request_body)
    }

    /// Executes a bytes processed throughput subscription
    fn component_bytes_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentBytesProcessedThroughputSubscription> {
        let request_body = ComponentBytesProcessedThroughputSubscription::build_query(
            component_bytes_processed_throughput_subscription::Variables { interval },
        );

        self.start::<ComponentBytesProcessedThroughputSubscription>(&request_body)
    }

    /// Executes an all component bytes processed throughputs subscription
    fn all_component_bytes_processed_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<AllComponentBytesProcessedThroughputsSubscription> {
        let request_body = AllComponentBytesProcessedThroughputsSubscription::build_query(
            all_component_bytes_processed_throughputs_subscription::Variables { interval },
        );

        self.start::<AllComponentBytesProcessedThroughputsSubscription>(&request_body)
    }
}
