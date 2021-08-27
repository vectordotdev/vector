use futures::stream::Stream;
use futures::StreamExt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use vector_core::event::Event;

pub struct Batcher<'a, St: ?Sized> {
    stream: &'a mut St,
}

impl<St> Unpin for Batcher<'_, St> where St: ?Sized + Unpin {}

impl<'a, St> Batcher<'a, St>
where
    St: ?Sized + Stream + Unpin,
{
    pub fn new(stream: &'a mut St) -> Self {
        Self { stream }
    }
}

impl<St> Future for Batcher<'_, St>
where
    St: ?Sized + Stream + Unpin,
    St::Item: Event,
{
    type Output = Option<St::Item>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.stream.poll_next_unpin(cx)
    }
}

// use crate::sinks::datadog::logs::log_api::common;
// use futures::Stream;
// use std::collections::HashMap;
// use std::hash::BuildHasherDefault;
// use std::pin::Pin;
// use std::task::{Context, Poll};
// use twox_hash::XxHash64;
// use vector_core::event::Event;

// const MAX_PAYLOAD_ARRAY: usize = 1_000;

// /// Batch incoming `Event` instances up for payload serialization
// ///
// /// Datadog Log api payloads have a few constraints. Each message must have no
// /// more than 1_000 members and payloads must not exceed 5Mb in size before
// /// compression. Every member in the payload must also ship with the same API
// /// key, meaning batches are constructed per-key. The API makes no restriction
// /// on how often we can call it, nor is there a minimum payload size.
// ///
// /// This structure confines itself to concerns about element totals and timing
// /// out if the stream of `Event`s for a particular key are slow.
// struct Batcher<'a> {
//     /// The default Datadog API key to use
//     ///
//     /// In some instances an `Event` will come in on the stream with an
//     /// associated API key. That API key is the one it'll get batched up by but
//     /// otherwise we will see `Event` instances with no associated key. In that
//     /// case we batch them by this default.
//     ///
//     /// Note that this is a `u64` and not a `Box<str>` or similar. This sink
//     /// stores all API keys in a slab and only materializes the actual API key
//     /// when needed.
//     default_api_key: u64,
//     /// The slab of API keys
//     ///
//     /// This slab holds the actual materialized API key in the form of a
//     /// `Box<str>`. This avoids having lots of little strings running around
//     /// with the downside of being an unbounded structure, in the present
//     /// implementation.
//     key_slab: HashMap<u64, Box<str>, BuildHasherDefault<XxHash64>>,
//     /// The batches of `Event` instances, sorted by API key
//     event_batches: HashMap<Box<str>, Vec<Event>, BuildHasherDefault<XxHash64>>,
//     /// The interior stream to wrap
//     inner: Stream<Item = Event> + 'a,
// }

// impl<'a> Batcher<'a> {
//     fn batch(default_api_key: Box<str>, input: impl Stream<Item = Event> + 'a) -> Self {
//         let mut key_slab = HashMap::default();
//         let default_key_id = common::hash(&default_api_key);
//         key_slab.insert(default_key_id, default_api_key);

//         Self {
//             default_api_key: default_key_id,
//             key_slab,
//             event_batches: HashMap::default(),
//             inner: Box::new(input),
//         }
//     }

//     /// Calculates and store the API key ID of an `Event`
//     ///
//     /// This function calculates the API key ID of a given `Event`. As a
//     /// side-effect it mutates internal state of the struct allowing callers to
//     /// use the ID to retrieve a `Box<str>` of the key at a later time.
//     fn register_key_id(&mut self, event: &Event) -> u64 {
//         if let Some(api_key) = event.metadata().datadog_api_key() {
//             let key = api_key.as_ref();
//             let key_hash = common::hash(key);
//             // TODO it'd be nice to avoid passing through String
//             self.key_slab
//                 .entry(key_hash)
//                 .or_insert_with(|| String::from(key).into_boxed_str());
//             key_hash
//         } else {
//             self.default_api_key
//         }
//     }
// }

// impl<'a> Stream for Batcher<'a> {
//     type Item = (Box<str>, Vec<Event>);

//     fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         unimplemented!()
//     }

//     //    fn size_hint(&self) -> (usize, Option<usize>) { ... }
// }
