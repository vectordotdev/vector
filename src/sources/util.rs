use futures::{Poll, Stream};

pub trait StreamExt<T>
where
    Self: Stream<Item = T> + Sized,
{
    fn filter_map_err<F, B>(self, f: F) -> FilterMapErr<Self, F>
    where
        F: FnMut(Self::Error) -> Option<B>,
        Self: Sized,
    {
        FilterMapErr::new(self, f)
    }
}

impl<T, S> StreamExt<T> for S where S: Stream<Item = T> + Sized {}

pub struct FilterMapErr<S, F> {
    inner: S,
    f: F,
}

impl<S, F> FilterMapErr<S, F> {
    pub fn new<B>(s: S, f: F) -> Self
    where
        S: Stream,
        F: FnMut(S::Error) -> Option<B>,
    {
        Self { inner: s, f: f }
    }
}

impl<S, F, B> Stream for FilterMapErr<S, F>
where
    S: Stream,
    F: FnMut(S::Error) -> Option<B>,
{
    type Item = S::Item;
    type Error = B;

    fn poll(&mut self) -> Poll<Option<S::Item>, B> {
        loop {
            match self.inner.poll() {
                Err(err) => {
                    if let Some(new_err) = (self.f)(err) {
                        return Err(new_err);
                    }
                }
                Ok(ok) => return Ok(ok),
            }
        }
    }
}
