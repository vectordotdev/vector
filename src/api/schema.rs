use juniper::{EmptyMutation, EmptySubscription, RootNode};

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

pub type Schema = RootNode<'static, Query, EmptyMutation<Context>, EmptySubscription<Context>>;

pub fn schema() -> Schema {
    Schema::new(Query, EmptyMutation::new(), EmptySubscription::new())
}
