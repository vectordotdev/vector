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

/// ComponentEventsProcessedThroughputBatchSubscription contains metrics on the number of events
/// that have been processed between `interval` samples, against specific components
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_events_processed_throughput_batch.graphql",
    response_derives = "Debug"
)]
pub struct ComponentEventsProcessedThroughputBatchSubscription;

/// ComponentEventsProcessedTotalSubscription contains metrics on the number of events
/// that have been processed by a Vector instance, against specific components
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_events_processed_total_batch.graphql",
    response_derives = "Debug"
)]
pub struct ComponentEventsProcessedTotalBatchSubscription;

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

/// ComponentBytesProcessedThroughputBatchSubscription contains metrics on the number of bytes
/// that have been processed between `interval` samples, against specific components
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_bytes_processed_throughput_batch.graphql",
    response_derives = "Debug"
)]
pub struct ComponentBytesProcessedThroughputBatchSubscription;

/// ComponentBytesProcessedTotalSubscription contains metrics on the number of bytes
/// that have been processed by a Vector instance, against a specific component
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_bytes_processed_total_batch.graphql",
    response_derives = "Debug"
)]
pub struct ComponentBytesProcessedTotalBatchSubscription;

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

    /// Executes an events processed throughput subscription
    fn bytes_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<BytesProcessedThroughputSubscription>;

    /// Executes a components events processed total metrics subscription
    fn component_events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsProcessedTotalSubscription>;

    /// Executes a components events processed total metrics batch subscription
    fn component_events_processed_total_batch_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsProcessedTotalBatchSubscription>;

    /// Executes a components events processed throughput metrics subscription
    fn component_events_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsProcessedThroughputSubscription>;

    /// Executes a components events processed throughput matcj metrics subscription
    fn component_events_processed_throughput_batch_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsProcessedThroughputBatchSubscription>;

    /// Executes a components bytes processed total metrics subscription
    fn component_bytes_processed_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentBytesProcessedTotalSubscription>;

    /// Executes a components bytes processed total metrics batch subscription
    fn component_bytes_processed_total_batch_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentBytesProcessedTotalBatchSubscription>;

    /// Executes a components bytes processed throughput metrics subscription
    fn component_bytes_processed_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentBytesProcessedThroughputSubscription>;

    /// Executes a components bytes processed throughput batch metrics subscription
    fn component_bytes_processed_throughput_batch_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentBytesProcessedThroughputBatchSubscription>;
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

    /// Executes a components events processed total metrics subscription
    fn component_events_processed_total_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentEventsProcessedTotalSubscription> {
        let request_body = ComponentEventsProcessedTotalSubscription::build_query(
            component_events_processed_total_subscription::Variables { interval },
        );

        self.start::<ComponentEventsProcessedTotalSubscription>(&request_body)
    }

    /// Executes a components events processed total batch metrics subscription
    fn component_events_processed_total_batch_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentEventsProcessedTotalBatchSubscription> {
        let request_body = ComponentEventsProcessedTotalBatchSubscription::build_query(
            component_events_processed_total_batch_subscription::Variables { interval },
        );

        self.start::<ComponentEventsProcessedTotalBatchSubscription>(&request_body)
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

    /// Executes a components events processed throughput batch metrics subscription
    fn component_events_processed_throughput_batch_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentEventsProcessedThroughputBatchSubscription> {
        let request_body = ComponentEventsProcessedThroughputBatchSubscription::build_query(
            component_events_processed_throughput_batch_subscription::Variables { interval },
        );

        self.start::<ComponentEventsProcessedThroughputBatchSubscription>(&request_body)
    }
    /// Executes a components bytes processed total metrics subscription
    fn component_bytes_processed_total_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentBytesProcessedTotalSubscription> {
        let request_body = ComponentBytesProcessedTotalSubscription::build_query(
            component_bytes_processed_total_subscription::Variables { interval },
        );

        self.start::<ComponentBytesProcessedTotalSubscription>(&request_body)
    }

    /// Executes a components bytes processed total metrics subscription
    fn component_bytes_processed_total_batch_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentBytesProcessedTotalBatchSubscription> {
        let request_body = ComponentBytesProcessedTotalBatchSubscription::build_query(
            component_bytes_processed_total_batch_subscription::Variables { interval },
        );

        self.start::<ComponentBytesProcessedTotalBatchSubscription>(&request_body)
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

    /// Executes a components bytes processed throughput batch metrics subscription
    fn component_bytes_processed_throughput_batch_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentBytesProcessedThroughputBatchSubscription> {
        let request_body = ComponentBytesProcessedThroughputBatchSubscription::build_query(
            component_bytes_processed_throughput_batch_subscription::Variables { interval },
        );

        self.start::<ComponentBytesProcessedThroughputBatchSubscription>(&request_body)
    }
}
