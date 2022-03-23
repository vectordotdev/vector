use async_graphql::{Object, SimpleObject, Subscription};
use chrono::{DateTime, Utc};
use tokio::time::Duration;
use tokio_stream::{wrappers::IntervalStream, Stream, StreamExt};

#[derive(SimpleObject)]
pub struct Heartbeat {
    utc: DateTime<Utc>,
}

impl Heartbeat {
    fn new() -> Self {
        Heartbeat { utc: Utc::now() }
    }
}

#[derive(Default)]
pub(super) struct HealthQuery;

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
        #[graphql(default = 1000, validator(minimum = 10, maximum = 60_000))] interval: i32,
    ) -> impl Stream<Item = Heartbeat> {
        IntervalStream::new(tokio::time::interval(Duration::from_millis(
            interval as u64,
        )))
        .map(|_| Heartbeat::new())
    }
}
