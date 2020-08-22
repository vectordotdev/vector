use chrono::{DateTime, Utc};
use juniper::{EmptyMutation, FieldError, RootNode};
use std::pin::Pin;
use tokio::{stream::Stream, time::Duration};

#[derive(Clone)]
pub struct Context {}

impl juniper::Context for Context {}
impl Context {
    pub fn new() -> Context {
        Context {}
    }
}

pub struct Query;

#[juniper::graphql_object(Context = Context)]
impl Query {
    async fn health() -> bool {
        true
    }
}

pub struct Subscription;

#[derive(Clone)]
struct Heartbeat {
    utc: DateTime<Utc>,
}

impl Heartbeat {
    fn new() -> Self {
        Heartbeat { utc: Utc::now() }
    }
}

#[juniper::graphql_object(Context = Context)]
impl Heartbeat {
    fn utc(&self) -> DateTime<Utc> {
        self.utc
    }
}

type HeartbeatStream = Pin<Box<dyn Stream<Item = Result<Heartbeat, FieldError>> + Send>>;

#[juniper::graphql_subscription(Context = Context)]
impl Subscription {
    async fn heartbeat(interval: Option<i32>) -> HeartbeatStream {
        let interval = match interval {
            Some(i) if (100..=60_000).contains(&i) => i as u64,
            _ => 1_000,
        };

        let stream =
            tokio::time::interval(Duration::from_millis(interval)).map(|_| Ok(Heartbeat::new()));

        Box::pin(stream)
    }
}

pub type Schema = RootNode<'static, Query, EmptyMutation<Context>, Subscription>;

pub fn schema() -> Schema {
    Schema::new(Query, EmptyMutation::new(), Subscription)
}
