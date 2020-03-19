use crate::Event;
use async_trait::async_trait;
use futures::Stream;

pub mod compat;

#[async_trait]
pub trait StreamingSink: Send + Sync + 'static {
    async fn run(
        &mut self,
        input: impl Stream<Item = Event> + Send + Sync + 'static,
    ) -> crate::Result<()>;
}
