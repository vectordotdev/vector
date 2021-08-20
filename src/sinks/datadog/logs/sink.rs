use crate::sinks::datadog::logs::config::Encoding;
use crate::sinks::util::encoding::{EncodingConfigWithDefault, EncodingConfiguration};
use async_trait::async_trait;
use futures::stream::{BoxStream, FuturesOrdered, FuturesUnordered};
use futures::StreamExt;
use snafu::Snafu;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::iter::FromIterator;
use tokio::time::{self, Duration};
use twox_hash::XxHash64;
use vector_core::event::{Event, LogEvent, Value};
use vector_core::sink::StreamSink;
use vector_core::ByteSizeOf;

#[derive(Debug, Snafu)]
pub enum BuildError {}

#[derive(Debug, Snafu)]
pub enum LogApiError {}

#[derive(Debug)]
pub struct LogApiBuilder {}

impl LogApiBuilder {
    pub fn build(self) -> Result<LogApi, BuildError> {
        unimplemented!()
        // Ok(LogApi {})
    }
}

const MAX_PAYLOAD_ARRAY: usize = 1_000;

#[derive(Debug)]
pub struct LogApi {
    /// The default Datadog API key to use
    ///
    /// In some instances an `Event` will come in on the stream with an
    /// associated API key. That API key is the one it'll get batched up by but
    /// otherwise we will see `Event` instances with no associated key. In that
    /// case we batch them by this default.
    ///
    /// Note that this is a `usize` and not a `Box<str>` or similar. This sink
    /// stores all API keys in a slab and only materializes the actual API key
    /// when needed.
    default_api_key: u64,
    /// The slab of API keys
    ///
    /// This slab holds the actual materialized API key in the form of a
    /// `Box<str>`. This avoids having lots of little strings running around
    /// with the downside of being an unbounded structure, in the present
    /// implementation.
    key_slab: HashMap<u64, Box<str>, BuildHasherDefault<XxHash64>>,
    /// The batches of `Event` instances, sorted by API key
    event_batches: HashMap<u64, Vec<Event>, BuildHasherDefault<XxHash64>>,
    /// The total number of seconds before a flush is forced
    ///
    /// This value sets the total number of seconds that are allowed to ellapse
    /// prior to a flush of all buffered `Event` instances.
    timeout_seconds: u64,
    /// The total number of bytes this struct is allowed to hold
    ///
    /// This value acts as a soft limit on the amount of bytes this struct is
    /// allowed to hold prior to a flush happening. This limit is soft as if an
    /// event comes in and would cause `bytes_stored` to eclipse this value
    /// we'll need to temporarily store that event while a flush happens.
    bytes_stored_limit: usize,
    /// Tracks the total in-memory bytes being held by this struct
    ///
    /// This value tells us how many bytes our buffered `Event` instances are
    /// consuming. Once this value is >= `bytes_stored_limit` a flush will be
    /// triggered.
    bytes_stored: usize,
    /// The "message" key for the global log schema
    log_schema_message_key: &'static str,
    /// The "timestamp" key for the global log schema
    log_schema_timestamp_key: &'static str,
    /// The "host" key for the global log schema
    log_schema_host_key: &'static str,
    /// The encoding of payloads
    ///
    /// This struct always generates JSON payloads. However we do, technically,
    /// allow the user to set the encoding to a single value -- JSON -- and this
    /// encoding comes with rules on sanitizing the payload which must be
    /// applied.
    encoding: EncodingConfigWithDefault<Encoding>,
}

#[derive(serde::Serialize)]
struct Payload {
    #[serde(flatten)]
    members: Vec<BTreeMap<String, Value>>,
}

impl LogApi {
    pub fn new() -> LogApiBuilder {
        LogApiBuilder {}
    }

    /// Calculates the API key ID of an `Event`
    ///
    /// This function calculates the API key ID of a given `Event`. As a
    /// side-effect it mutates internal state of the struct allowing callers to
    /// use the ID to retrieve a `Box<str>` of the key at a later time.
    fn register_key_id(&mut self, event: &Event) -> u64 {
        if let Some(api_key) = event.metadata().datadog_api_key() {
            let key = api_key.as_ref();
            let key_hash = hash(key);
            // TODO it'd be nice to avoid passing through String
            self.key_slab
                .entry(key_hash)
                .or_insert_with(|| String::from(key).into_boxed_str());
            key_hash
        } else {
            self.default_api_key
        }
    }

    fn has_space(&self, id: u64) -> bool {
        if let Some(arr) = self.event_batches.get(&id) {
            arr.len() < MAX_PAYLOAD_ARRAY
        } else {
            true
        }
    }

    /// Stores the `Event` in its batch
    ///
    /// This function stores the `Event` in its `id` appropriate batch. Caller
    /// MUST confirm that there is space in the batch for the `Event` by calling
    /// `has_space`, which must return true.
    ///
    /// # Panics
    ///
    /// This function will panic if there is no space in the underlying batch.
    fn store_event(&mut self, id: u64, event: Event) {
        let arr = self
            .event_batches
            .entry(id)
            .or_insert_with(|| Vec::with_capacity(MAX_PAYLOAD_ARRAY));
        assert!(arr.len() + 1 <= MAX_PAYLOAD_ARRAY);
        arr.push(event);
    }

    // fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
    //     let log = event.as_mut_log();

    //     if self.log_schema_message_key != "message" {
    //         if let Some(message) = log.remove(self.log_schema_message_key) {
    //             log.insert_flat("message", message);
    //         }
    //     }

    //     if self.log_schema_timestamp_key != "date" {
    //         if let Some(timestamp) = log.remove(self.log_schema_timestamp_key) {
    //             log.insert_flat("date", timestamp);
    //         }
    //     }

    //     if self.log_schema_host_key != "host" {
    //         if let Some(host) = log.remove(self.log_schema_host_key) {
    //             log.insert_flat("host", host);
    //         }
    //     }

    //     self.encoding.apply_rules(&mut event);

    //     let (fields, metadata) = event.into_log().into_parts();
    //     let json_event = json!(fields);

    //     Some(PartitionInnerBuffer::new(json_event, Arc::clone(api_key)))
    // }

    async fn flush_batch(&self, api_key_id: u64, batch: Vec<Event>) -> Result<(), ()> {
        // TODO pull apart the `batch` and plop its btrees Payload for
        // serialization. We might need to serialize in a wonky way if we go
        // over the 5Mb limit.
        //
        // OPEN QUESTION: how does ack'ing work? Presumably that's held in the
        // metadata of the `Event` but I'm not sure when to trigger it.

        // let payload = Payload { members };
        // self.encode(batch);

        unimplemented!()
    }

    /// Flush the `Event` batches out to Datadog API
    async fn flush(&mut self) -> Result<(), ()> {
        // The Datadog API makes no claim on how many requests we can make at
        // one time. We rely on the http client to handle retries and what
        // not. This implementation stacks up futures for each key present in
        // the batches and runs them unordered, returning when the last
        // completes.

        // NOTE I would prefer to map directly over the Drain without going
        // through a vec but mixing ownership confused me greatly, since we need
        // to call back to `self` to flush the batch.
        let batches: Vec<(u64, Vec<Event>)> = self.event_batches.drain().collect();
        let futures = batches
            .into_iter()
            .map(|(key_id, batch)| self.flush_batch(key_id, batch));
        // let mut flushes = FuturesUnordered::from_iter(futures);
        // while let Some(_) = flushes.next().await {}

        Ok(())
    }
}

#[inline]
fn hash(input: &str) -> u64 {
    let mut hasher = XxHash64::default();
    hasher.write(input.as_bytes());
    hasher.finish()
}

// NOTE likely implementation for #8491
fn rename_key(from_key: &'static str, to_key: &'static str, log: &mut LogEvent) {
    if from_key != to_key {
        if let Some(val) = log.remove(from_key) {
            log.insert_flat(to_key, val);
        }
    }
}

#[async_trait]
impl StreamSink for LogApi {
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let encoding = self.encoding.clone();
        let message_key = self.log_schema_message_key;
        let timestamp_key = self.log_schema_timestamp_key;
        let host_key = self.log_schema_host_key;

        let mut input = input.map(|mut event| {
            let log = event.as_mut_log();
            rename_key(message_key, "message", log);
            rename_key(timestamp_key, "date", log);
            rename_key(host_key, "host", log);
            encoding.apply_rules(&mut event);
            event
        });

        let mut interval = time::interval(Duration::from_secs(self.timeout_seconds));
        loop {
            tokio::select! {
                _ = interval.tick() => self.flush().await?,
                Some(event) = input.next() => {
                    let key_id = self.register_key_id(&event);
                    let event_size = event.size_of();
                    if self.bytes_stored_limit < self.bytes_stored + event_size {
                        // We've gone over the bytes limit and must flush our
                        // batches. As a future optimization, since we haven't
                        // passed the timeout yet, we might consider only
                        // flushing a single large batch. As of this writing the
                        // batch structure doesn't allow sorting by size so we
                        // just ship the whole thing.
                        self.flush().await?;
                    }
                    if !self.has_space(key_id) {
                        self.flush().await?;
                    }
                    self.store_event(key_id, event);

                    // TODO how do we ack?
                }
            }
        }
    }
}
