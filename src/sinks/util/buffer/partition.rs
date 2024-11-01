use vector_lib::ByteSizeOf;

use super::super::{
    batch::{Batch, BatchConfig, BatchError, PushResult},
    ElementCount,
};
use crate::sinks::util::{Merged, SinkBatchSettings};

pub trait Partition<K> {
    fn partition(&self) -> K;
}
#[derive(Debug)]
pub struct PartitionBuffer<T, K> {
    inner: T,
    key: Option<K>,
}

#[derive(Debug, Clone)]
pub struct PartitionInnerBuffer<T, K> {
    pub(self) inner: T,
    key: K,
}

impl<T, K> PartitionBuffer<T, K> {
    pub const fn new(inner: T) -> Self {
        Self { inner, key: None }
    }
}

impl<T, K> Batch for PartitionBuffer<T, K>
where
    T: Batch,
    K: Clone,
{
    type Input = PartitionInnerBuffer<T::Input, K>;
    type Output = PartitionInnerBuffer<T::Output, K>;

    fn get_settings_defaults<D: SinkBatchSettings + Clone>(
        config: BatchConfig<D, Merged>,
    ) -> Result<BatchConfig<D, Merged>, BatchError> {
        T::get_settings_defaults(config)
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

    fn fresh(&self) -> Self {
        Self::new(self.inner.fresh())
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
    pub const fn new(inner: T, key: K) -> Self {
        Self { inner, key }
    }

    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
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

impl<T: ByteSizeOf, K> ByteSizeOf for PartitionInnerBuffer<T, K> {
    // This ignores the size of the key, as it does not represent actual data size.
    fn size_of(&self) -> usize {
        self.inner.size_of()
    }

    fn allocated_bytes(&self) -> usize {
        self.inner.allocated_bytes()
    }
}

impl<T: ElementCount, K> ElementCount for PartitionInnerBuffer<T, K> {
    fn element_count(&self) -> usize {
        self.inner.element_count()
    }
}
