use async_graphql::{validators::IntRange, Object, SimpleObject, Subscription};
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

#[derive(Default)]
pub struct HealthSubscription;

#[Subscription]
impl HealthSubscription {
    /// Heartbeat, containing the UTC timestamp of the last server-sent payload
    async fn heartbeat(
        &self,
        #[arg(default = 1000, validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Heartbeat> {
        tokio::time::interval(Duration::from_millis(interval as u64)).map(|_| Heartbeat::new())
    }
}
