use std::{iter, sync::Arc};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use codecs::StreamDecodingError;
use tokio_util::codec::Decoder as _;
use vector_core::{internal_event::EventsReceived, ByteSizeOf};

use crate::{codecs::Decoder, config::log_schema, event::BatchNotifier, event::Event};

pub fn decode_message<'a>(
    mut decoder: Decoder,
    source_type: &'static str,
    message: &[u8],
    timestamp: Option<DateTime<Utc>>,
    batch: &'a Option<Arc<BatchNotifier>>,
) -> impl Iterator<Item = Event> + 'a {
    let schema = log_schema();

    let mut buffer = BytesMut::with_capacity(message.len());
    buffer.extend_from_slice(message);

    iter::from_fn(move || loop {
        break match decoder.decode_eof(&mut buffer) {
            Ok(Some((events, _))) => {
                let count = events.len();
                Some(
                    events
                        .into_iter()
                        .map(move |mut event| {
                            if let Event::Log(ref mut log) = event {
                                log.try_insert(schema.source_type_key(), Bytes::from(source_type));
                                if let Some(timestamp) = timestamp {
                                    log.try_insert(schema.timestamp_key(), timestamp);
                                }
                            }
                            event
                        })
                        .fold_finally(
                            0,
                            |size, event: &Event| size + event.size_of(),
                            move |byte_size| emit!(EventsReceived { byte_size, count }),
                        ),
                )
            }
            Err(error) => {
                // Error is logged by `crate::codecs::Decoder`, no further handling
                // is needed here.
                if error.can_continue() {
                    continue;
                }
                None
            }
            Ok(None) => None,
        };
    })
    .flatten()
    .map(move |event| event.with_batch_notifier_option(batch))
}

trait FoldFinallyExt: Sized {
    /// This adapter applies the `folder` function to every element in
    /// the iterator, much as `Iterator::fold` does. However, instead
    /// of returning the resulting folded value, it calls the
    /// `finally` function after the last element. This function
    /// returns an iterator over the original values.
    fn fold_finally<A, Fo, Fi>(
        self,
        initial: A,
        folder: Fo,
        finally: Fi,
    ) -> FoldFinally<Self, A, Fo, Fi>;
}

impl<I: Iterator + Sized> FoldFinallyExt for I {
    fn fold_finally<A, Fo, Fi>(
        self,
        initial: A,
        folder: Fo,
        finally: Fi,
    ) -> FoldFinally<Self, A, Fo, Fi> {
        FoldFinally {
            inner: self,
            accumulator: initial,
            folder,
            finally,
        }
    }
}

struct FoldFinally<I, A, Fo, Fi> {
    inner: I,
    accumulator: A,
    folder: Fo,
    finally: Fi,
}

impl<I, A, Fo, Fi> Iterator for FoldFinally<I, A, Fo, Fi>
where
    I: Iterator,
    A: Copy,
    Fo: FnMut(A, &I::Item) -> A,
    Fi: Fn(A),
{
    type Item = I::Item;
    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next() {
            Some(item) => {
                self.accumulator = (self.folder)(self.accumulator, &item);
                Some(item)
            }
            None => {
                (self.finally)(self.accumulator);
                None
            }
        }
    }
}
