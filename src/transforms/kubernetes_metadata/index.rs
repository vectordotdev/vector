use async_trait::async_trait;
use futures::future::BoxFuture;

use crate::kubernetes::{
    self as k8s,
    any_resource::{AnyResource, SharedAnyResource},
    state::evmap::Indexer,
};

pub struct Manager<T, I>
where
    T: Send,
    I: Indexer<SharedAnyResource> + Send,
{
    inner: T,
    index_state: k8s::state::evmap::Writer<SharedAnyResource, I>,
}

impl<T, I> Manager<T, I>
where
    T: Send,
    I: Indexer<SharedAnyResource> + Send,
{
    pub fn new(inner: T, index_state: k8s::state::evmap::Writer<SharedAnyResource, I>) -> Self {
        Self { inner, index_state }
    }
}

#[async_trait]
impl<T, I> k8s::state::Write for Manager<T, I>
where
    T: k8s::state::Write<Item = SharedAnyResource> + Send,
    I: Indexer<SharedAnyResource> + Send,
{
    type Item = AnyResource;

    async fn add(&mut self, item: Self::Item) {
        let item = SharedAnyResource::from(item);
        self.inner.add(item.clone()).await;
        self.index_state.add(item).await;
    }

    async fn update(&mut self, item: Self::Item) {
        let item = SharedAnyResource::from(item);
        self.inner.update(item.clone()).await;
        self.index_state.update(item).await;
    }

    async fn delete(&mut self, item: Self::Item) {
        let item = SharedAnyResource::from(item);
        self.inner.delete(item.clone()).await;
        self.index_state.delete(item).await;
    }

    async fn resync(&mut self) {
        self.inner.resync().await;
        self.index_state.resync().await;
    }
}

#[async_trait]
impl<T, I> k8s::state::MaintainedWrite for Manager<T, I>
where
    T: k8s::state::MaintainedWrite<Item = SharedAnyResource> + Send,
    I: Indexer<SharedAnyResource> + Send,
{
    fn maintenance_request(&mut self) -> Option<BoxFuture<'_, ()>> {
        k8s::state::merge_maintenance_requests(
            self.index_state.maintenance_request(),
            self.inner.maintenance_request(),
        )
    }

    async fn perform_maintenance(&mut self) {
        self.inner.perform_maintenance().await;
        self.index_state.perform_maintenance().await;
    }
}
