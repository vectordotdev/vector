pub trait BatchData<T> {
    type Batch;

    /// The number of items in the batch
    fn len(&self) -> usize;

    /// Return the current batch, and reset any internal state
    fn take_batch(&mut self) -> Self::Batch;

    /// Add a single item to the batch
    fn push_item(&mut self, item: T);

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> BatchData<T> for Vec<T> {
    type Batch = Self;

    fn len(&self) -> usize {
        self.len()
    }
    fn take_batch(&mut self) -> Self::Batch {
        std::mem::take(self)
    }
    fn push_item(&mut self, item: T) {
        self.push(item);
    }
}

pub struct BatchReduce<F, S> {
    reducer: F,
    state: S,
    len: usize,
}
impl<F, S> BatchReduce<F, S>
where
    S: Default,
{
    pub fn new(reducer: F) -> BatchReduce<F, S> {
        BatchReduce {
            reducer,
            state: S::default(),
            len: 0,
        }
    }
}
impl<F, S, T> BatchData<T> for BatchReduce<F, S>
where
    F: FnMut(&mut S, T),
    S: Default,
{
    type Batch = S;

    fn len(&self) -> usize {
        self.len
    }

    fn take_batch(&mut self) -> Self::Batch {
        self.len = 0;
        std::mem::take(&mut self.state)
    }

    fn push_item(&mut self, item: T) {
        self.len += 1;
        (self.reducer)(&mut self.state, item);
    }
}
