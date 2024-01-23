use async_stream::stream;
use futures::{Stream, StreamExt};
use std::time::Duration;

#[derive(Default)]
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
    expiration_interval: Duration,
    // called for each event
    mut map_fn: M,
    // called periodically to allow expiring internal state
    mut expiration_fn: E,
    // called once at the end of the input stream
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

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_simple() {
        let input = futures::stream::iter([1, 2, 3]);

        let map_fn = |state: &mut i32, event, emitter: &mut Emitter<i32>| {
            *state += event;
            emitter.emit(*state);
        };
        let expiration_fn = |_state: &mut i32, _emitter: &mut Emitter<i32>| {
            // do nothing
        };
        let flush_fn = |state: &mut i32, emitter: &mut Emitter<i32>| {
            emitter.emit(*state);
        };
        let stream: Vec<i32> = map_with_expiration(
            0_i32,
            input,
            Duration::from_secs(100),
            map_fn,
            expiration_fn,
            flush_fn,
        )
        .take(4)
        .collect()
        .await;

        assert_eq!(vec![1, 3, 6, 6], stream);
    }

    #[tokio::test]
    async fn test_expiration() {
        // an input that never ends (to test expiration)
        let input = futures::stream::iter([1, 2, 3]).chain(futures::stream::pending());

        let map_fn = |state: &mut i32, event, emitter: &mut Emitter<i32>| {
            *state += event;
            emitter.emit(*state);
        };
        let expiration_fn = |state: &mut i32, emitter: &mut Emitter<i32>| {
            emitter.emit(*state);
        };
        let flush_fn = |_state: &mut i32, _emitter: &mut Emitter<i32>| {
            // do nothing
        };
        let stream: Vec<i32> = map_with_expiration(
            0_i32,
            input,
            Duration::from_secs(1),
            map_fn,
            expiration_fn,
            flush_fn,
        )
        .take(4)
        .collect()
        .await;

        assert_eq!(vec![1, 3, 6, 6], stream);
    }
}
