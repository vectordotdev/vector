use super::super::{Batch, BatchSize, PushResult};

#[derive(Clone)]
pub struct VecBuffer<T> {
    batch: Vec<T>,
    max_events: usize,
}

impl<T> VecBuffer<T> {
    pub fn new(settings: BatchSize) -> Self {
        Self::new_with_max(settings.events)
    }

    fn new_with_max(max_events: usize) -> Self {
        let batch = Vec::with_capacity(max_events);
        Self { batch, max_events }
    }
}

impl<T> Batch for VecBuffer<T> {
    type Input = T;
    type Output = Vec<T>;

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        if self.batch.len() >= self.max_events {
            PushResult::Overflow(item)
        } else {
            self.batch.push(item);
            PushResult::Ok(self.batch.len() >= self.max_events)
        }
    }

    fn is_empty(&self) -> bool {
        self.batch.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new_with_max(self.max_events)
    }

    fn finish(self) -> Self::Output {
        self.batch
    }

    fn num_items(&self) -> usize {
        self.batch.len()
    }
}
