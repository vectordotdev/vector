//! Metrics queries/subscriptions.

use crate::BoxedSubscription;
use graphql_client::GraphQLQuery;

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

/// ReceivedEventsTotalSubscription contains metrics on the number of events
/// that have been accepted for processing by a Vector instance.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/received_events_total.graphql",
    response_derives = "Debug"
)]
pub struct ReceivedEventsTotalSubscription;

/// ReceivedEventsThroughputSubscription contains metrics on the number of events
/// that have been accepted for processing between `interval` samples.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/received_events_throughput.graphql",
    response_derives = "Debug"
)]
pub struct ReceivedEventsThroughputSubscription;

/// EventsOutTotalSubscription contains metrics on the number of events
/// that have been emitted by a Vector instance.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/events_out_total.graphql",
    response_derives = "Debug"
)]
pub struct EventsOutTotalSubscription;

/// EventsOutThroughputSubscription contains metrics on the number of events
/// that have been emitted between `interval` samples.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/events_out_throughput.graphql",
    response_derives = "Debug"
)]
pub struct EventsOutThroughputSubscription;

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

/// ComponentEventsOutThroughputsSubscription contains metrics on the number of events
/// that have been emitted between `interval` samples, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_events_out_throughputs.graphql",
    response_derives = "Debug"
)]
pub struct ComponentEventsOutThroughputsSubscription;

/// ComponentEventsOutTotalsSubscription contains metrics on the number of events
/// that have been emitted by a Vector instance, against specific components.
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_events_out_totals.graphql",
    response_derives = "Debug"
)]
pub struct ComponentEventsOutTotalsSubscription;

/// Extension methods for metrics subscriptions
pub trait MetricsSubscriptionExt {
    /// Executes an uptime metrics subscription.
    fn uptime_subscription(&self) -> crate::BoxedSubscription<UptimeSubscription>;

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

    /// Executes a received events total metrics subscription
    fn received_events_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ReceivedEventsTotalSubscription>;

    /// Executes a received events throughput subscription.
    fn received_events_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ReceivedEventsThroughputSubscription>;

    /// Executes an events out metrics subscription.
    fn events_out_total_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<EventsOutTotalSubscription>;

    /// Executes an events out throughput subscription.
    fn events_out_throughput_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<EventsOutThroughputSubscription>;

    /// Executes an component events processed totals subscription
    fn component_processed_events_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentProcessedEventsTotalsSubscription>;

    /// Executes an component events processed throughputs subscription.
    fn component_processed_events_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentProcessedEventsThroughputsSubscription>;

    /// Executes an component bytes processed totals subscription.
    fn component_processed_bytes_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentProcessedBytesTotalsSubscription>;

    /// Executes an component bytes processed throughputs subscription.
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

    /// Executes an component events out totals subscription.
    fn component_events_out_totals_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsOutTotalsSubscription>;

    /// Executes an component events in throughputs subscription.
    fn component_events_out_throughputs_subscription(
        &self,
        interval: i64,
    ) -> crate::BoxedSubscription<ComponentEventsOutThroughputsSubscription>;
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

    /// Executes a received events total metrics subscription.
    fn received_events_total_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ReceivedEventsTotalSubscription> {
        let request_body = ReceivedEventsTotalSubscription::build_query(
            received_events_total_subscription::Variables { interval },
        );

        self.start::<ReceivedEventsTotalSubscription>(&request_body)
    }

    /// Executes a received events throughput subscription.
    fn received_events_throughput_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ReceivedEventsThroughputSubscription> {
        let request_body = ReceivedEventsThroughputSubscription::build_query(
            received_events_throughput_subscription::Variables { interval },
        );

        self.start::<ReceivedEventsThroughputSubscription>(&request_body)
    }

    /// Executes an events out metrics subscription.
    fn events_out_total_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<EventsOutTotalSubscription> {
        let request_body =
            EventsOutTotalSubscription::build_query(events_out_total_subscription::Variables {
                interval,
            });

        self.start::<EventsOutTotalSubscription>(&request_body)
    }

    /// Executes an events out throughput subscription.
    fn events_out_throughput_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<EventsOutThroughputSubscription> {
        let request_body = EventsOutThroughputSubscription::build_query(
            events_out_throughput_subscription::Variables { interval },
        );

        self.start::<EventsOutThroughputSubscription>(&request_body)
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

    /// Executes an all component events out totals subscription.
    fn component_events_out_totals_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentEventsOutTotalsSubscription> {
        let request_body = ComponentEventsOutTotalsSubscription::build_query(
            component_events_out_totals_subscription::Variables { interval },
        );

        self.start::<ComponentEventsOutTotalsSubscription>(&request_body)
    }

    /// Executes an all component events out throughputs subscription.
    fn component_events_out_throughputs_subscription(
        &self,
        interval: i64,
    ) -> BoxedSubscription<ComponentEventsOutThroughputsSubscription> {
        let request_body = ComponentEventsOutThroughputsSubscription::build_query(
            component_events_out_throughputs_subscription::Variables { interval },
        );

        self.start::<ComponentEventsOutThroughputsSubscription>(&request_body)
    }
}
