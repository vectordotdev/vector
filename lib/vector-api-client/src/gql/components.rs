use std::fmt;

use async_trait::async_trait;
use graphql_client::GraphQLQuery;

use crate::{BoxedSubscription, QueryResult};

/// Components query for returning sources, transforms, and sinks
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/queries/components.graphql",
    response_derives = "Debug"
)]
pub struct ComponentsQuery;

/// Components subscription for notification when a component has been added
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_added.graphql",
    response_derives = "Debug"
)]
pub struct ComponentAddedSubscription;

/// Components subscription for notification when a component has been removed
#[derive(GraphQLQuery, Debug, Copy, Clone)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/component_removed.graphql",
    response_derives = "Debug"
)]
pub struct ComponentRemovedSubscription;

#[async_trait]
pub trait ComponentsQueryExt {
    async fn components_query(&self, first: i64) -> crate::QueryResult<ComponentsQuery>;
}

#[async_trait]
impl ComponentsQueryExt for crate::Client {
    async fn components_query(&self, first: i64) -> QueryResult<ComponentsQuery> {
        let request_body = ComponentsQuery::build_query(components_query::Variables { first });
        self.query::<ComponentsQuery>(&request_body).await
    }
}

pub trait ComponentsSubscriptionExt {
    fn component_added(&self) -> crate::BoxedSubscription<ComponentAddedSubscription>;
    fn component_removed(&self) -> crate::BoxedSubscription<ComponentRemovedSubscription>;
}

#[async_trait]
impl ComponentsSubscriptionExt for crate::SubscriptionClient {
    /// Subscription for when a component has been added
    fn component_added(&self) -> BoxedSubscription<ComponentAddedSubscription> {
        let request_body =
            ComponentAddedSubscription::build_query(component_added_subscription::Variables);

        self.start::<ComponentAddedSubscription>(&request_body)
    }

    /// Subscription for when a component has been removed
    fn component_removed(&self) -> BoxedSubscription<ComponentRemovedSubscription> {
        let request_body =
            ComponentRemovedSubscription::build_query(component_removed_subscription::Variables);

        self.start::<ComponentRemovedSubscription>(&request_body)
    }
}

impl components_query::ComponentsQueryComponentsEdgesNodeOn {
    pub fn processed_events_total(&self) -> i64 {
        match self {
            components_query::ComponentsQueryComponentsEdgesNodeOn::Source(s) => s
                .metrics
                .processed_events_total
                .as_ref()
                .map(|p| p.processed_events_total as i64)
                .unwrap_or(0),
            components_query::ComponentsQueryComponentsEdgesNodeOn::Transform(t) => t
                .metrics
                .processed_events_total
                .as_ref()
                .map(|p| p.processed_events_total as i64)
                .unwrap_or(0),
            components_query::ComponentsQueryComponentsEdgesNodeOn::Sink(s) => s
                .metrics
                .processed_events_total
                .as_ref()
                .map(|p| p.processed_events_total as i64)
                .unwrap_or(0),
        }
    }

    pub fn processed_bytes_total(&self) -> i64 {
        match self {
            components_query::ComponentsQueryComponentsEdgesNodeOn::Source(s) => s
                .metrics
                .processed_bytes_total
                .as_ref()
                .map(|p| p.processed_bytes_total as i64)
                .unwrap_or(0),
            components_query::ComponentsQueryComponentsEdgesNodeOn::Transform(t) => t
                .metrics
                .processed_bytes_total
                .as_ref()
                .map(|p| p.processed_bytes_total as i64)
                .unwrap_or(0),
            components_query::ComponentsQueryComponentsEdgesNodeOn::Sink(s) => s
                .metrics
                .processed_bytes_total
                .as_ref()
                .map(|p| p.processed_bytes_total as i64)
                .unwrap_or(0),
        }
    }

    pub fn received_events_total(&self) -> i64 {
        match self {
            components_query::ComponentsQueryComponentsEdgesNodeOn::Source(s) => s
                .metrics
                .received_events_total
                .as_ref()
                .map(|p| p.received_events_total as i64)
                .unwrap_or(0),
            components_query::ComponentsQueryComponentsEdgesNodeOn::Transform(t) => t
                .metrics
                .received_events_total
                .as_ref()
                .map(|p| p.received_events_total as i64)
                .unwrap_or(0),
            components_query::ComponentsQueryComponentsEdgesNodeOn::Sink(s) => s
                .metrics
                .received_events_total
                .as_ref()
                .map(|p| p.received_events_total as i64)
                .unwrap_or(0),
        }
    }

    pub fn sent_events_total(&self) -> i64 {
        match self {
            components_query::ComponentsQueryComponentsEdgesNodeOn::Source(s) => s
                .metrics
                .sent_events_total
                .as_ref()
                .map(|p| p.sent_events_total as i64)
                .unwrap_or(0),
            components_query::ComponentsQueryComponentsEdgesNodeOn::Transform(t) => t
                .metrics
                .sent_events_total
                .as_ref()
                .map(|p| p.sent_events_total as i64)
                .unwrap_or(0),
            components_query::ComponentsQueryComponentsEdgesNodeOn::Sink(s) => s
                .metrics
                .sent_events_total
                .as_ref()
                .map(|p| p.sent_events_total as i64)
                .unwrap_or(0),
        }
    }
}

impl fmt::Display for components_query::ComponentsQueryComponentsEdgesNodeOn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let res = match self {
            components_query::ComponentsQueryComponentsEdgesNodeOn::Source(_) => "source",
            components_query::ComponentsQueryComponentsEdgesNodeOn::Transform(_) => "transform",
            components_query::ComponentsQueryComponentsEdgesNodeOn::Sink(_) => "sink",
        };

        write!(f, "{}", res)
    }
}

impl fmt::Display for component_added_subscription::ComponentAddedSubscriptionComponentAddedOn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let res = match self {
            component_added_subscription::ComponentAddedSubscriptionComponentAddedOn::Source => {
                "source"
            }
            component_added_subscription::ComponentAddedSubscriptionComponentAddedOn::Transform => {
                "transform"
            }
            component_added_subscription::ComponentAddedSubscriptionComponentAddedOn::Sink => {
                "sink"
            }
        };

        write!(f, "{}", res)
    }
}

impl fmt::Display
    for component_removed_subscription::ComponentRemovedSubscriptionComponentRemovedOn
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let res = match self {
            component_removed_subscription::ComponentRemovedSubscriptionComponentRemovedOn::Source => {
                "source"
            }
            component_removed_subscription::ComponentRemovedSubscriptionComponentRemovedOn::Transform => {
                "transform"
            }
            component_removed_subscription::ComponentRemovedSubscriptionComponentRemovedOn::Sink => {
                "sink"
            }
        };

        write!(f, "{}", res)
    }
}
