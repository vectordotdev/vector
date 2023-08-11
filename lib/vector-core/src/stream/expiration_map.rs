use async_stream::stream;
use futures::{Stream, StreamExt};
use std::time::Duration;

pub struct Emitter<T> {
    values: Vec<T>,
}

impl<T> Emitter<T> {
    pub fn new() -> Self {
        Self { values: vec![] }
    }
    pub fn emit(&mut self, value: T) {
        self.values.push(value);
    }
}

/// Similar to `stream.filter_map(..).flatten(..)` but also allows checking for expired events
/// and flushing when the input stream ends.
pub fn map_with_expiration<S, T, M, E, F>(
    initial_state: S,
    input: impl Stream<Item = T> + 'static,
    mut map_fn: M,
    mut expiration_fn: E,
    expiration_interval: Duration,
    mut flush_fn: F,
) -> impl Stream<Item = T>
where
    M: FnMut(&mut S, T, &mut Emitter<T>),
    E: FnMut(&mut S, &mut Emitter<T>),
    F: FnMut(&mut S, &mut Emitter<T>),
{
    let mut state = initial_state;
    let mut flush_stream = tokio::time::interval(expiration_interval);

    Box::pin(stream! {
        futures_util::pin_mut!(input);
              loop {
                let mut emitter = Emitter::<T>::new();
                let done = tokio::select! {
                    _ = flush_stream.tick() => {
                        expiration_fn(&mut state, &mut emitter);
                        false
                    }
                    maybe_event = input.next() => {
                      match maybe_event {
                        None => {
                                flush_fn(&mut state, &mut emitter);
                          true
                        }
                        Some(event) => {
                                map_fn(&mut state, event, &mut emitter);
                          false
                        }
                      }
                    }
                };
                yield futures::stream::iter(emitter.values.into_iter());
                if done { break }
              }

    })
    .flatten()
}
