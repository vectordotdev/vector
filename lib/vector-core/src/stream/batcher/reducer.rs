use super::data::BatchData;

/// A batch reducer based on a function.
pub struct FunctionReducer<F, S> {
    reducer: F,
    state: S,
    len: usize,
}

impl<F, S> FunctionReducer<F, S>
where
    S: Default,
{
    /// Creates a `FunctionReducer` with the given reducer function and default state.
    pub fn with_default_state(reducer: F) -> Self {
        Self {
            reducer,
            state: S::default(),
            len: 0,
        }
    }
}

impl<F, S, T> BatchData<T> for FunctionReducer<F, S>
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
