use async_graphql::{Object, SimpleObject};
use chrono::{DateTime, Utc};

use tokio::stream::{Stream, StreamExt};
use tokio::time::Duration;

#[SimpleObject]
pub struct Heartbeat {
    utc: DateTime<Utc>,
}

impl Heartbeat {
    fn new() -> Self {
        Heartbeat { utc: Utc::now() }
    }
}

#[derive(Default)]
pub struct HealthQuery;

#[Object]
impl HealthQuery {
    /// Returns `true` to denote the GraphQL server is reachable
    async fn health(&self) -> bool {
        true
    }
}

/// Returns a stream of heartbeats, at `interval` milliseconds
pub fn heartbeat_stream(interval: i32) -> impl Stream<Item = Heartbeat> {
    tokio::time::interval(Duration::from_millis(interval as u64)).map(|_| Heartbeat::new())
}
