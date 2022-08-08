/// A type that can store data for a batch.
pub trait BatchData<T> {
    /// Type of the batch object that gets returned.
    type Batch;

    /// Returns the number of elements in the batch, also referred to as its 'length'.
    fn len(&self) -> usize;

    /// Takes all of the elements in the batch and returns them.
    ///
    /// This typically resets any internal batch state.
    fn take_batch(&mut self) -> Self::Batch;

    /// Adds a single item into the batch.
    fn push_item(&mut self, item: T);

    /// Returns `true` if the batch contains no elements.
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
