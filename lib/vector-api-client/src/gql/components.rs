use crate::{BoxedSubscription, QueryResult};
use async_trait::async_trait;
use graphql_client::GraphQLQuery;
use serde::export::Formatter;

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
    async fn components_query(&self) -> crate::QueryResult<ComponentsQuery>;
}

#[async_trait]
impl ComponentsQueryExt for crate::Client {
    async fn components_query(&self) -> QueryResult<ComponentsQuery> {
        let request_body = ComponentsQuery::build_query(components_query::Variables);
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

impl std::fmt::Display for components_query::ComponentsQueryComponentsOn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let res = match self {
            components_query::ComponentsQueryComponentsOn::Source => "source",
            components_query::ComponentsQueryComponentsOn::Transform => "transform",
            components_query::ComponentsQueryComponentsOn::Sink => "sink",
        };

        write!(f, "{}", res)
    }
}

impl std::fmt::Display
    for component_added_subscription::ComponentAddedSubscriptionComponentAddedOn
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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

impl std::fmt::Display
    for component_removed_subscription::ComponentRemovedSubscriptionComponentRemovedOn
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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
