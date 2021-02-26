use super::super::batch::{Batch, BatchConfig, BatchError, BatchMaker, BatchSettings, PushResult};
use std::marker::PhantomData;

pub trait Partition<K> {
    fn partition(&self) -> K;
}

#[derive(Debug)]
pub struct PartitionBuffer<T, K> {
    inner: T,
    key: Option<K>,
}

pub struct PartitionBufferMaker<M, K> {
    batch_maker: M,
    _phantom: PhantomData<K>,
}

impl<M, K> BatchMaker for PartitionBufferMaker<M, K>
where
    M: BatchMaker,
    K: Clone,
{
    type Batch = PartitionBuffer<M::Batch, K>;
    fn new_batch(&self) -> Self::Batch {
        Self::Batch::new(self.batch_maker.new_batch())
    }
}

#[derive(Debug, Clone)]
pub struct PartitionInnerBuffer<T, K> {
    pub(self) inner: T,
    key: K,
}

impl<T, K> PartitionBuffer<T, K> {
    pub fn new(inner: T) -> Self {
        Self { inner, key: None }
    }

    pub fn maker<M>(batch_maker: M) -> PartitionBufferMaker<M, K>
    where
        M: BatchMaker<Batch = T>,
    {
        PartitionBufferMaker {
            batch_maker,
            _phantom: PhantomData::default(),
        }
    }
}

impl<T, K> Batch for PartitionBuffer<T, K>
where
    T: Batch,
    K: Clone,
{
    type Input = PartitionInnerBuffer<T::Input, K>;
    type Output = PartitionInnerBuffer<T::Output, K>;

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(T::get_settings_defaults(config, defaults.into())?.into())
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        let key = item.key;
        match self.inner.push(item.inner) {
            PushResult::Ok(full) => {
                self.key = Some(key);
                PushResult::Ok(full)
            }
            PushResult::Overflow(inner) => PushResult::Overflow(Self::Input { inner, key }),
        }
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn finish(mut self) -> Self::Output {
        let key = self.key.take().unwrap();
        let inner = self.inner.finish();
        PartitionInnerBuffer { inner, key }
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}

impl<T, K> PartitionInnerBuffer<T, K> {
    pub fn new(inner: T, key: K) -> Self {
        Self { inner, key }
    }

    pub fn into_parts(self) -> (T, K) {
        (self.inner, self.key)
    }
}

impl<T, K> Partition<K> for PartitionInnerBuffer<T, K>
where
    K: Clone,
{
    fn partition(&self) -> K {
        self.key.clone()
    }
}
