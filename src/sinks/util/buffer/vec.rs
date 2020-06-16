use super::super::{Batch, BatchSettings};

#[derive(Clone)]
pub struct VecBuffer<T> {
    batch: Vec<T>,
    max_events: usize,
}

impl<T> VecBuffer<T> {
    pub fn new(settings: BatchSettings) -> Self {
        Self {
            batch: Vec::with_capacity(settings.size),
            max_events: settings.size,
        }
    }
}

impl<T> Batch for VecBuffer<T> {
    type Input = T;
    type Output = Vec<T>;

    fn len(&self) -> usize {
        self.batch.len()
    }

    fn push(&mut self, item: Self::Input) {
        self.batch.push(item)
    }

    fn is_empty(&self) -> bool {
        self.batch.is_empty()
    }

    fn fresh(&self) -> Self {
        Self {
            batch: Vec::with_capacity(self.max_events),
            max_events: self.max_events,
        }
    }

    fn finish(self) -> Self::Output {
        self.batch
    }

    fn num_items(&self) -> usize {
        self.batch.len()
    }
}
