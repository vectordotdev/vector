use chrono;
use graphql_client::GraphQLQuery;
use serde::{Deserialize, Serialize};

type DateTime = chrono::DateTime<chrono::Utc>;

#[derive(GraphQLQuery, Deserialize, Serialize)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/queries/health.graphql",
    response_derives = "Debug"
)]
pub struct HealthQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/heartbeat.graphql",
    response_derives = "Debug"
)]
pub struct HeartbeatSubscription;
