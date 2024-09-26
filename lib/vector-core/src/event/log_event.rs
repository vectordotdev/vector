use crate::event::util::log::all_fields_skip_array_elements;
use bytes::Bytes;
use chrono::Utc;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt::Debug,
    iter::FromIterator,
    mem::size_of,
    num::NonZeroUsize,
    sync::Arc,
};

use crossbeam_utils::atomic::AtomicCell;
use lookup::{lookup_v2::TargetPath, metadata_path, path, PathPrefix};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize, Serializer};
use vector_common::{
    byte_size_of::ByteSizeOf,
    internal_event::{OptionalTag, TaggedEventsSent},
    json_size::{JsonSize, NonZeroJsonSize},
    request_metadata::GetEventCountTags,
    EventDataEq,
};
use vrl::path::{parse_target_path, OwnedTargetPath, PathParseError};
use vrl::{event_path, owned_value_path};

use super::{
    estimated_json_encoded_size_of::EstimatedJsonEncodedSizeOf,
    finalization::{BatchNotifier, EventFinalizer},
    metadata::EventMetadata,
    util, EventFinalizers, Finalizable, KeyString, ObjectMap, Value,
};
use crate::config::LogNamespace;
use crate::config::{log_schema, telemetry};
use crate::event::util::log::{all_fields, all_metadata_fields};
use crate::event::MaybeAsLogMut;

static VECTOR_SOURCE_TYPE_PATH: Lazy<Option<OwnedTargetPath>> = Lazy::new(|| {
    Some(OwnedTargetPath::metadata(owned_value_path!(
        "vector",
        "source_type"
    )))
});

#[derive(Debug, Deserialize)]
struct Inner {
    #[serde(flatten)]
    fields: Value,

    #[serde(skip)]
    size_cache: AtomicCell<Option<NonZeroUsize>>,

    #[serde(skip)]
    json_encoded_size_cache: AtomicCell<Option<NonZeroJsonSize>>,
}

impl Inner {
    fn invalidate(&self) {
        self.size_cache.store(None);
        self.json_encoded_size_cache.store(None);
    }

    fn as_value(&self) -> &Value {
        &self.fields
    }
}

impl ByteSizeOf for Inner {
    fn size_of(&self) -> usize {
        self.size_cache
            .load()
            .unwrap_or_else(|| {
                let size = size_of::<Self>() + self.allocated_bytes();
                // The size of self will always be non-zero, and
                // adding the allocated bytes cannot make it overflow
                // since `usize` has a range the same as pointer
                // space. Hence, the expect below cannot fail.
                let size = NonZeroUsize::new(size).expect("Size cannot be zero");
                self.size_cache.store(Some(size));
                size
            })
            .into()
    }

    fn allocated_bytes(&self) -> usize {
        self.fields.allocated_bytes()
    }
}

impl EstimatedJsonEncodedSizeOf for Inner {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.json_encoded_size_cache
            .load()
            .unwrap_or_else(|| {
                let size = self.fields.estimated_json_encoded_size_of();
                let size = NonZeroJsonSize::new(size).expect("Size cannot be zero");

                self.json_encoded_size_cache.store(Some(size));
                size
            })
            .into()
    }
}

impl Clone for Inner {
    fn clone(&self) -> Self {
        Self {
            fields: self.fields.clone(),
            // This clone is only ever used in combination with
            // `Arc::make_mut`, so don't bother fetching the size
            // cache to copy it since it will be invalidated anyways.
            size_cache: None.into(),

            // This clone is only ever used in combination with
            // `Arc::make_mut`, so don't bother fetching the size
            // cache to copy it since it will be invalidated anyways.
            json_encoded_size_cache: None.into(),
        }
    }
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            // **IMPORTANT:** Due to numerous legacy reasons this **must** be a Map variant.
            fields: Value::Object(Default::default()),
            size_cache: Default::default(),
            json_encoded_size_cache: Default::default(),
        }
    }
}

impl From<Value> for Inner {
    fn from(fields: Value) -> Self {
        Self {
            fields,
            size_cache: Default::default(),
            json_encoded_size_cache: Default::default(),
        }
    }
}

impl PartialEq for Inner {
    fn eq(&self, other: &Self) -> bool {
        self.fields.eq(&other.fields)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct LogEvent {
    #[serde(flatten)]
    inner: Arc<Inner>,

    #[serde(skip)]
    metadata: EventMetadata,
}

impl LogEvent {
    /// This used to be the implementation for `LogEvent::from(&'str)`, but this is now only
    /// valid for `LogNamespace::Legacy`
    pub fn from_str_legacy(msg: impl Into<String>) -> Self {
        let mut log = LogEvent::default();
        log.maybe_insert(log_schema().message_key_target_path(), msg.into());

        if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
            log.insert(timestamp_key, Utc::now());
        }

        log
    }

    /// This used to be the implementation for `LogEvent::from(Bytes)`, but this is now only
    /// valid for `LogNamespace::Legacy`
    pub fn from_bytes_legacy(msg: &Bytes) -> Self {
        Self::from_str_legacy(String::from_utf8_lossy(msg.as_ref()).to_string())
    }

    pub fn value(&self) -> &Value {
        self.inner.as_ref().as_value()
    }

    pub fn value_mut(&mut self) -> &mut Value {
        let result = Arc::make_mut(&mut self.inner);
        // We MUST invalidate the inner size cache when making a
        // mutable copy, since the _next_ action will modify the data.
        result.invalidate();
        &mut result.fields
    }

    pub fn metadata(&self) -> &EventMetadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut EventMetadata {
        &mut self.metadata
    }

    /// This detects the log namespace used at runtime by checking for the existence
    /// of the read-only "vector" metadata, which only exists (and is required to exist)
    /// with the `Vector` log namespace.
    pub fn namespace(&self) -> LogNamespace {
        if self.contains((PathPrefix::Metadata, path!("vector"))) {
            LogNamespace::Vector
        } else {
            LogNamespace::Legacy
        }
    }
}

impl ByteSizeOf for LogEvent {
    fn allocated_bytes(&self) -> usize {
        self.inner.size_of() + self.metadata.allocated_bytes()
    }
}

impl Finalizable for LogEvent {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.metadata.take_finalizers()
    }
}

impl EstimatedJsonEncodedSizeOf for LogEvent {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.inner.estimated_json_encoded_size_of()
    }
}

impl GetEventCountTags for LogEvent {
    fn get_tags(&self) -> TaggedEventsSent {
        let source = if telemetry().tags().emit_source {
            self.metadata().source_id().cloned().into()
        } else {
            OptionalTag::Ignored
        };

        let service = if telemetry().tags().emit_service {
            self.get_by_meaning("service")
                .map(|value| value.to_string_lossy().to_string())
                .into()
        } else {
            OptionalTag::Ignored
        };

        TaggedEventsSent { source, service }
    }
}

impl LogEvent {
    #[must_use]
    pub fn new_with_metadata(metadata: EventMetadata) -> Self {
        Self {
            inner: Default::default(),
            metadata,
        }
    }

    ///  Create a `LogEvent` from a `Value` and `EventMetadata`
    pub fn from_parts(value: Value, metadata: EventMetadata) -> Self {
        Self {
            inner: Arc::new(value.into()),
            metadata,
        }
    }

    ///  Create a `LogEvent` from an `ObjectMap` and `EventMetadata`
    pub fn from_map(map: ObjectMap, metadata: EventMetadata) -> Self {
        let inner = Arc::new(Inner::from(Value::Object(map)));
        Self { inner, metadata }
    }

    /// Convert a `LogEvent` into a tuple of its components
    pub fn into_parts(mut self) -> (Value, EventMetadata) {
        self.value_mut();

        let value = Arc::try_unwrap(self.inner)
            .unwrap_or_else(|_| unreachable!("inner fields already cloned after owning"))
            .fields;
        let metadata = self.metadata;
        (value, metadata)
    }

    #[must_use]
    pub fn with_batch_notifier(mut self, batch: &BatchNotifier) -> Self {
        self.metadata = self.metadata.with_batch_notifier(batch);
        self
    }

    #[must_use]
    pub fn with_batch_notifier_option(mut self, batch: &Option<BatchNotifier>) -> Self {
        self.metadata = self.metadata.with_batch_notifier_option(batch);
        self
    }

    pub fn add_finalizer(&mut self, finalizer: EventFinalizer) {
        self.metadata.add_finalizer(finalizer);
    }

    /// Parse the specified `path` and if there are no parsing errors, attempt to get a reference to a value.
    /// # Errors
    /// Will return an error if path parsing failed.
    pub fn parse_path_and_get_value(
        &self,
        path: impl AsRef<str>,
    ) -> Result<Option<&Value>, PathParseError> {
        parse_target_path(path.as_ref()).map(|path| self.get(&path))
    }

    #[allow(clippy::needless_pass_by_value)] // TargetPath is always a reference
    pub fn get<'a>(&self, key: impl TargetPath<'a>) -> Option<&Value> {
        match key.prefix() {
            PathPrefix::Event => self.inner.fields.get(key.value_path()),
            PathPrefix::Metadata => self.metadata.value().get(key.value_path()),
        }
    }

    /// Retrieves the value of a field based on it's meaning.
    /// This will first check if the value has previously been dropped. It is worth being
    /// aware that if the field has been dropped and then somehow re-added, we still fetch
    /// the dropped value here.
    pub fn get_by_meaning(&self, meaning: impl AsRef<str>) -> Option<&Value> {
        self.metadata().dropped_field(&meaning).or_else(|| {
            self.metadata()
                .schema_definition()
                .meaning_path(meaning.as_ref())
                .and_then(|path| self.get(path))
        })
    }

    /// Retrieves the mutable value of a field based on it's meaning.
    /// Note that this does _not_ check the dropped fields, unlike `get_by_meaning`, since the
    /// purpose of the mutable reference is to be able to modify the value and modifying the dropped
    /// fields has no effect on the resulting event.
    pub fn get_mut_by_meaning(&mut self, meaning: impl AsRef<str>) -> Option<&mut Value> {
        Arc::clone(self.metadata.schema_definition())
            .meaning_path(meaning.as_ref())
            .and_then(|path| self.get_mut(path))
    }

    /// Retrieves the target path of a field based on the specified `meaning`.
    pub fn find_key_by_meaning(&self, meaning: impl AsRef<str>) -> Option<&OwnedTargetPath> {
        self.metadata()
            .schema_definition()
            .meaning_path(meaning.as_ref())
    }

    #[allow(clippy::needless_pass_by_value)] // TargetPath is always a reference
    pub fn get_mut<'a>(&mut self, path: impl TargetPath<'a>) -> Option<&mut Value> {
        match path.prefix() {
            PathPrefix::Event => self.value_mut().get_mut(path.value_path()),
            PathPrefix::Metadata => self.metadata.value_mut().get_mut(path.value_path()),
        }
    }

    #[allow(clippy::needless_pass_by_value)] // TargetPath is always a reference
    pub fn contains<'a>(&self, path: impl TargetPath<'a>) -> bool {
        match path.prefix() {
            PathPrefix::Event => self.value().contains(path.value_path()),
            PathPrefix::Metadata => self.metadata.value().contains(path.value_path()),
        }
    }

    /// Parse the specified `path` and if there are no parsing errors, attempt to insert the specified `value`.
    ///
    /// # Errors
    /// Will return an error if path parsing failed.
    pub fn parse_path_and_insert(
        &mut self,
        path: impl AsRef<str>,
        value: impl Into<Value>,
    ) -> Result<Option<Value>, PathParseError> {
        let target_path = parse_target_path(path.as_ref())?;
        Ok(self.insert(&target_path, value))
    }

    #[allow(clippy::needless_pass_by_value)] // TargetPath is always a reference
    pub fn insert<'a>(
        &mut self,
        path: impl TargetPath<'a>,
        value: impl Into<Value>,
    ) -> Option<Value> {
        match path.prefix() {
            PathPrefix::Event => self.value_mut().insert(path.value_path(), value.into()),
            PathPrefix::Metadata => self
                .metadata
                .value_mut()
                .insert(path.value_path(), value.into()),
        }
    }

    pub fn maybe_insert<'a>(&mut self, path: Option<impl TargetPath<'a>>, value: impl Into<Value>) {
        if let Some(path) = path {
            self.insert(path, value);
        }
    }

    // deprecated - using this means the schema is unknown
    pub fn try_insert<'a>(&mut self, path: impl TargetPath<'a>, value: impl Into<Value>) {
        if !self.contains(path.clone()) {
            self.insert(path, value);
        }
    }

    /// Rename a key
    ///
    /// If `to_key` already exists in the structure its value will be overwritten.
    pub fn rename_key<'a>(&mut self, from: impl TargetPath<'a>, to: impl TargetPath<'a>) {
        if let Some(val) = self.remove(from) {
            self.insert(to, val);
        }
    }

    pub fn remove<'a>(&mut self, path: impl TargetPath<'a>) -> Option<Value> {
        self.remove_prune(path, false)
    }

    #[allow(clippy::needless_pass_by_value)] // TargetPath is always a reference
    pub fn remove_prune<'a>(&mut self, path: impl TargetPath<'a>, prune: bool) -> Option<Value> {
        match path.prefix() {
            PathPrefix::Event => self.value_mut().remove(path.value_path(), prune),
            PathPrefix::Metadata => self.metadata.value_mut().remove(path.value_path(), prune),
        }
    }

    pub fn keys(&self) -> Option<impl Iterator<Item = KeyString> + '_> {
        match &self.inner.fields {
            Value::Object(map) => Some(util::log::keys(map)),
            _ => None,
        }
    }

    /// If the event root value is a map, build and return an iterator to event field and value pairs.
    /// TODO: Ideally this should return target paths to be consistent with other `LogEvent` methods.
    pub fn all_event_fields(
        &self,
    ) -> Option<impl Iterator<Item = (KeyString, &Value)> + Serialize> {
        self.as_map().map(all_fields)
    }

    /// Similar to [`LogEvent::all_event_fields`], but doesn't traverse individual array elements.
    pub fn all_event_fields_skip_array_elements(
        &self,
    ) -> Option<impl Iterator<Item = (KeyString, &Value)> + Serialize> {
        self.as_map().map(all_fields_skip_array_elements)
    }

    /// If the metadata root value is a map, build and return an iterator to metadata field and value pairs.
    /// TODO: Ideally this should return target paths to be consistent with other `LogEvent` methods.
    pub fn all_metadata_fields(
        &self,
    ) -> Option<impl Iterator<Item = (KeyString, &Value)> + Serialize> {
        match self.metadata.value() {
            Value::Object(metadata_map) => Some(all_metadata_fields(metadata_map)),
            _ => None,
        }
    }

    /// Returns an iterator of all fields if the value is an Object. Otherwise, a single field is
    /// returned with a "message" key. Field names that are could be interpreted as alternate paths
    /// (i.e. containing periods, square brackets, etc) are quoted.
    pub fn convert_to_fields(&self) -> impl Iterator<Item = (KeyString, &Value)> + Serialize {
        if let Some(map) = self.as_map() {
            util::log::all_fields(map)
        } else {
            util::log::all_fields_non_object_root(self.value())
        }
    }

    /// Returns an iterator of all fields if the value is an Object. Otherwise, a single field is
    /// returned with a "message" key. Field names are not quoted.
    pub fn convert_to_fields_unquoted(
        &self,
    ) -> impl Iterator<Item = (KeyString, &Value)> + Serialize {
        if let Some(map) = self.as_map() {
            util::log::all_fields_unquoted(map)
        } else {
            util::log::all_fields_non_object_root(self.value())
        }
    }

    pub fn is_empty_object(&self) -> bool {
        if let Some(map) = self.as_map() {
            map.is_empty()
        } else {
            false
        }
    }

    pub fn as_map(&self) -> Option<&ObjectMap> {
        match self.value() {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_map_mut(&mut self) -> Option<&mut ObjectMap> {
        match self.value_mut() {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    /// Merge all fields specified at `fields` from `incoming` to `current`.
    /// Note that `fields` containing dots and other special characters will be treated as a single segment.
    pub fn merge(&mut self, mut incoming: LogEvent, fields: &[impl AsRef<str>]) {
        for field in fields {
            let field_path = event_path!(field.as_ref());
            let Some(incoming_val) = incoming.remove(field_path) else {
                continue;
            };
            match self.get_mut(field_path) {
                None => {
                    self.insert(field_path, incoming_val);
                }
                Some(current_val) => current_val.merge(incoming_val),
            }
        }
        self.metadata.merge(incoming.metadata);
    }
}

/// Log Namespace utility methods. These can only be used when an event has a
/// valid schema definition set (which should be on every event in transforms and sinks).
impl LogEvent {
    /// Fetches the "message" path of the event. This is either from the "message" semantic meaning (Vector namespace)
    /// or from the message key set on the "Global Log Schema" (Legacy namespace).
    pub fn message_path(&self) -> Option<&OwnedTargetPath> {
        match self.namespace() {
            LogNamespace::Vector => self.find_key_by_meaning("message"),
            LogNamespace::Legacy => log_schema().message_key_target_path(),
        }
    }

    /// Fetches the "timestamp" path of the event. This is either from the "timestamp" semantic meaning (Vector namespace)
    /// or from the timestamp key set on the "Global Log Schema" (Legacy namespace).
    pub fn timestamp_path(&self) -> Option<&OwnedTargetPath> {
        match self.namespace() {
            LogNamespace::Vector => self.find_key_by_meaning("timestamp"),
            LogNamespace::Legacy => log_schema().timestamp_key_target_path(),
        }
    }

    /// Fetches the `host` path of the event. This is either from the "host" semantic meaning (Vector namespace)
    /// or from the host key set on the "Global Log Schema" (Legacy namespace).
    pub fn host_path(&self) -> Option<&OwnedTargetPath> {
        match self.namespace() {
            LogNamespace::Vector => self.find_key_by_meaning("host"),
            LogNamespace::Legacy => log_schema().host_key_target_path(),
        }
    }

    /// Fetches the `source_type` path of the event. This is either from the `source_type` Vector metadata field (Vector namespace)
    /// or from the `source_type` key set on the "Global Log Schema" (Legacy namespace).
    pub fn source_type_path(&self) -> Option<&OwnedTargetPath> {
        match self.namespace() {
            LogNamespace::Vector => VECTOR_SOURCE_TYPE_PATH.as_ref(),
            LogNamespace::Legacy => log_schema().source_type_key_target_path(),
        }
    }

    /// Fetches the `message` of the event. This is either from the "message" semantic meaning (Vector namespace)
    /// or from the message key set on the "Global Log Schema" (Legacy namespace).
    pub fn get_message(&self) -> Option<&Value> {
        match self.namespace() {
            LogNamespace::Vector => self.get_by_meaning("message"),
            LogNamespace::Legacy => log_schema()
                .message_key_target_path()
                .and_then(|key| self.get(key)),
        }
    }

    /// Fetches the `timestamp` of the event. This is either from the "timestamp" semantic meaning (Vector namespace)
    /// or from the timestamp key set on the "Global Log Schema" (Legacy namespace).
    pub fn get_timestamp(&self) -> Option<&Value> {
        match self.namespace() {
            LogNamespace::Vector => self.get_by_meaning("timestamp"),
            LogNamespace::Legacy => log_schema()
                .timestamp_key_target_path()
                .and_then(|key| self.get(key)),
        }
    }

    /// Removes the `timestamp` from the event. This is either from the "timestamp" semantic meaning (Vector namespace)
    /// or from the timestamp key set on the "Global Log Schema" (Legacy namespace).
    pub fn remove_timestamp(&mut self) -> Option<Value> {
        self.timestamp_path()
            .cloned()
            .and_then(|key| self.remove(&key))
    }

    /// Fetches the `host` of the event. This is either from the "host" semantic meaning (Vector namespace)
    /// or from the host key set on the "Global Log Schema" (Legacy namespace).
    pub fn get_host(&self) -> Option<&Value> {
        match self.namespace() {
            LogNamespace::Vector => self.get_by_meaning("host"),
            LogNamespace::Legacy => log_schema()
                .host_key_target_path()
                .and_then(|key| self.get(key)),
        }
    }

    /// Fetches the `source_type` of the event. This is either from the `source_type` Vector metadata field (Vector namespace)
    /// or from the `source_type` key set on the "Global Log Schema" (Legacy namespace).
    pub fn get_source_type(&self) -> Option<&Value> {
        match self.namespace() {
            LogNamespace::Vector => self.get(metadata_path!("vector", "source_type")),
            LogNamespace::Legacy => log_schema()
                .source_type_key_target_path()
                .and_then(|key| self.get(key)),
        }
    }
}

impl MaybeAsLogMut for LogEvent {
    fn maybe_as_log_mut(&mut self) -> Option<&mut LogEvent> {
        Some(self)
    }
}

impl EventDataEq for LogEvent {
    fn event_data_eq(&self, other: &Self) -> bool {
        self.inner.fields == other.inner.fields && self.metadata.event_data_eq(&other.metadata)
    }
}

#[cfg(any(test, feature = "test"))]
mod test_utils {
    use super::*;

    // these rely on the global log schema, which is no longer supported when using the
    // "LogNamespace::Vector" namespace.
    // The tests that rely on this are testing the "Legacy" log namespace. As these
    // tests are updated, they should be migrated away from using these implementations
    // to make it more clear which namespace is being used

    impl From<Bytes> for LogEvent {
        fn from(message: Bytes) -> Self {
            let mut log = LogEvent::default();
            log.maybe_insert(log_schema().message_key_target_path(), message);
            if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
                log.insert(timestamp_key, Utc::now());
            }
            log
        }
    }

    impl From<&str> for LogEvent {
        fn from(message: &str) -> Self {
            message.to_owned().into()
        }
    }

    impl From<String> for LogEvent {
        fn from(message: String) -> Self {
            Bytes::from(message).into()
        }
    }
}

impl From<Value> for LogEvent {
    fn from(value: Value) -> Self {
        Self::from_parts(value, EventMetadata::default())
    }
}

impl From<ObjectMap> for LogEvent {
    fn from(map: ObjectMap) -> Self {
        Self::from_parts(Value::Object(map), EventMetadata::default())
    }
}

impl From<HashMap<KeyString, Value>> for LogEvent {
    fn from(map: HashMap<KeyString, Value>) -> Self {
        Self::from_parts(
            Value::Object(map.into_iter().collect::<ObjectMap>()),
            EventMetadata::default(),
        )
    }
}

impl TryFrom<serde_json::Value> for LogEvent {
    type Error = crate::Error;

    fn try_from(map: serde_json::Value) -> Result<Self, Self::Error> {
        match map {
            serde_json::Value::Object(fields) => Ok(LogEvent::from(
                fields
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect::<ObjectMap>(),
            )),
            _ => Err(crate::Error::from(
                "Attempted to convert non-Object JSON into a LogEvent.",
            )),
        }
    }
}

impl TryInto<serde_json::Value> for LogEvent {
    type Error = crate::Error;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        Ok(serde_json::to_value(&self.inner.fields)?)
    }
}

#[cfg(any(test, feature = "test"))]
impl<T> std::ops::Index<T> for LogEvent
where
    T: AsRef<str>,
{
    type Output = Value;

    fn index(&self, key: T) -> &Value {
        self.parse_path_and_get_value(key.as_ref())
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("Key is not found: {:?}", key.as_ref()))
    }
}

impl<K, V> Extend<(K, V)> for LogEvent
where
    K: AsRef<str>,
    V: Into<Value>,
{
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            if let Ok(path) = parse_target_path(k.as_ref()) {
                self.insert(&path, v.into());
            }
        }
    }
}

// Allow converting any kind of appropriate key/value iterator directly into a LogEvent.
impl<K: AsRef<str>, V: Into<Value>> FromIterator<(K, V)> for LogEvent {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut log_event = Self::default();
        log_event.extend(iter);
        log_event
    }
}

impl Serialize for LogEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value().serialize(serializer)
    }
}

// Tracing owned target paths used for tracing to log event conversions.
struct TracingTargetPaths {
    pub(crate) timestamp: OwnedTargetPath,
    pub(crate) kind: OwnedTargetPath,
    pub(crate) module_path: OwnedTargetPath,
    pub(crate) level: OwnedTargetPath,
    pub(crate) target: OwnedTargetPath,
}

/// Lazily initialized singleton.
static TRACING_TARGET_PATHS: Lazy<TracingTargetPaths> = Lazy::new(|| TracingTargetPaths {
    timestamp: OwnedTargetPath::event(owned_value_path!("timestamp")),
    kind: OwnedTargetPath::event(owned_value_path!("metadata", "kind")),
    level: OwnedTargetPath::event(owned_value_path!("metadata", "level")),
    module_path: OwnedTargetPath::event(owned_value_path!("metadata", "module_path")),
    target: OwnedTargetPath::event(owned_value_path!("metadata", "target")),
});

impl From<&tracing::Event<'_>> for LogEvent {
    fn from(event: &tracing::Event<'_>) -> Self {
        let now = chrono::Utc::now();
        let mut maker = LogEvent::default();
        event.record(&mut maker);

        let mut log = maker;
        log.insert(&TRACING_TARGET_PATHS.timestamp, now);

        let meta = event.metadata();
        log.insert(
            &TRACING_TARGET_PATHS.kind,
            if meta.is_event() {
                Value::Bytes("event".to_string().into())
            } else if meta.is_span() {
                Value::Bytes("span".to_string().into())
            } else {
                Value::Null
            },
        );
        log.insert(&TRACING_TARGET_PATHS.level, meta.level().to_string());
        log.insert(
            &TRACING_TARGET_PATHS.module_path,
            meta.module_path()
                .map_or(Value::Null, |mp| Value::Bytes(mp.to_string().into())),
        );
        log.insert(&TRACING_TARGET_PATHS.target, meta.target().to_string());
        log
    }
}

/// Note that `tracing::field::Field` containing dots and other special characters will be treated as a single segment.
impl tracing::field::Visit for LogEvent {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.insert(event_path!(field.name()), value.to_string());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn Debug) {
        self.insert(event_path!(field.name()), format!("{value:?}"));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.insert(event_path!(field.name()), value);
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        let field_path = event_path!(field.name());
        let converted: Result<i64, _> = value.try_into();
        match converted {
            Ok(value) => self.insert(field_path, value),
            Err(_) => self.insert(field_path, value.to_string()),
        };
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.insert(event_path!(field.name()), value);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::open_fixture;
    use lookup::event_path;
    use uuid::Version;
    use vrl::{btreemap, value};

    // The following two tests assert that renaming a key has no effect if the
    // keys are equivalent, whether the key exists in the log or not.
    #[test]
    fn rename_key_flat_equiv_exists() {
        let value = value!({
            one: 1,
            two: 2
        });

        let mut base = LogEvent::from_parts(value.clone(), EventMetadata::default());
        base.rename_key(event_path!("one"), event_path!("one"));
        let (actual_fields, _) = base.into_parts();

        assert_eq!(value, actual_fields);
    }
    #[test]
    fn rename_key_flat_equiv_not_exists() {
        let value = value!({
            one: 1,
            two: 2
        });

        let mut base = LogEvent::from_parts(value.clone(), EventMetadata::default());
        base.rename_key(event_path!("three"), event_path!("three"));
        let (actual_fields, _) = base.into_parts();

        assert_eq!(value, actual_fields);
    }
    // Assert that renaming a key has no effect if the key does not originally
    // exist in the log, when the to -> from keys are not identical.
    #[test]
    fn rename_key_flat_not_exists() {
        let value = value!({
            one: 1,
            two: 2
        });

        let mut base = LogEvent::from_parts(value.clone(), EventMetadata::default());
        base.rename_key(event_path!("three"), event_path!("four"));
        let (actual_fields, _) = base.into_parts();

        assert_eq!(value, actual_fields);
    }
    // Assert that renaming a key has the effect of moving the value from one
    // key name to another if the key exists.
    #[test]
    fn rename_key_flat_no_overlap() {
        let value = value!({
            one: 1,
            two: 2
        });

        let mut expected_value = value.clone();
        let one = expected_value.remove("one", true).unwrap();
        expected_value.insert("three", one);

        let mut base = LogEvent::from_parts(value, EventMetadata::default());
        base.rename_key(event_path!("one"), event_path!("three"));
        let (actual_fields, _) = base.into_parts();

        assert_eq!(expected_value, actual_fields);
    }
    // Assert that renaming a key has the effect of moving the value from one
    // key name to another if the key exists and will overwrite another key if
    // it exists.
    #[test]
    fn rename_key_flat_overlap() {
        let value = value!({
            one: 1,
            two: 2
        });

        let mut expected_value = value.clone();
        let val = expected_value.remove("one", true).unwrap();
        expected_value.insert("two", val);

        let mut base = LogEvent::from_parts(value, EventMetadata::default());
        base.rename_key(event_path!("one"), event_path!("two"));
        let (actual_value, _) = base.into_parts();

        assert_eq!(expected_value, actual_value);
    }

    #[test]
    fn insert() {
        let mut log = LogEvent::default();

        let old = log.insert("foo", "foo");

        assert_eq!(log.get("foo"), Some(&"foo".into()));
        assert_eq!(old, None);
    }

    #[test]
    fn insert_existing() {
        let mut log = LogEvent::default();
        log.insert("foo", "foo");

        let old = log.insert("foo", "bar");

        assert_eq!(log.get("foo"), Some(&"bar".into()));
        assert_eq!(old, Some("foo".into()));
    }

    #[test]
    fn try_insert() {
        let mut log = LogEvent::default();

        log.try_insert("foo", "foo");

        assert_eq!(log.get("foo"), Some(&"foo".into()));
    }

    #[test]
    fn try_insert_existing() {
        let mut log = LogEvent::default();
        log.insert("foo", "foo");

        log.try_insert("foo", "bar");

        assert_eq!(log.get("foo"), Some(&"foo".into()));
    }

    #[test]
    fn try_insert_dotted() {
        let mut log = LogEvent::default();

        log.try_insert("foo.bar", "foo");

        assert_eq!(log.get("foo.bar"), Some(&"foo".into()));
        assert_eq!(log.get(event_path!("foo.bar")), None);
    }

    #[test]
    fn try_insert_existing_dotted() {
        let mut log = LogEvent::default();
        log.insert("foo.bar", "foo");

        log.try_insert("foo.bar", "bar");

        assert_eq!(log.get("foo.bar"), Some(&"foo".into()));
        assert_eq!(log.get(event_path!("foo.bar")), None);
    }

    #[test]
    fn try_insert_flat() {
        let mut log = LogEvent::default();

        log.try_insert(event_path!("foo"), "foo");

        assert_eq!(log.get(event_path!("foo")), Some(&"foo".into()));
    }

    #[test]
    fn try_insert_flat_existing() {
        let mut log = LogEvent::default();
        log.insert(event_path!("foo"), "foo");

        log.try_insert(event_path!("foo"), "bar");

        assert_eq!(log.get(event_path!("foo")), Some(&"foo".into()));
    }

    #[test]
    fn try_insert_flat_dotted() {
        let mut log = LogEvent::default();

        log.try_insert(event_path!("foo.bar"), "foo");

        assert_eq!(log.get(event_path!("foo.bar")), Some(&"foo".into()));
        assert_eq!(log.get("foo.bar"), None);
    }

    #[test]
    fn try_insert_flat_existing_dotted() {
        let mut log = LogEvent::default();
        log.insert(event_path!("foo.bar"), "foo");

        log.try_insert(event_path!("foo.bar"), "bar");

        assert_eq!(log.get(event_path!("foo.bar")), Some(&"foo".into()));
        assert_eq!(log.get("foo.bar"), None);
    }

    // This test iterates over the `tests/data/fixtures/log_event` folder and:
    //
    //   * Ensures the EventLog parsed from bytes and turned into a
    //   serde_json::Value are equal to the item being just plain parsed as
    //   json.
    //
    // Basically: This test makes sure we aren't mutilating any content users
    // might be sending.
    #[test]
    fn json_value_to_vector_log_event_to_json_value() {
        const FIXTURE_ROOT: &str = "tests/data/fixtures/log_event";

        std::fs::read_dir(FIXTURE_ROOT)
            .unwrap()
            .for_each(|fixture_file| match fixture_file {
                Ok(fixture_file) => {
                    let path = fixture_file.path();
                    tracing::trace!(?path, "Opening.");
                    let serde_value = open_fixture(&path).unwrap();

                    let vector_value = LogEvent::try_from(serde_value.clone()).unwrap();
                    let serde_value_again: serde_json::Value = vector_value.try_into().unwrap();

                    assert_eq!(serde_value, serde_value_again);
                }
                _ => panic!("This test should never read Err'ing test fixtures."),
            });
    }

    fn assert_merge_value(
        current: impl Into<Value>,
        incoming: impl Into<Value>,
        expected: impl Into<Value>,
    ) {
        let mut merged = current.into();
        merged.merge(incoming.into());
        assert_eq!(merged, expected.into());
    }

    #[test]
    fn merge_value_works_correctly() {
        assert_merge_value("hello ", "world", "hello world");

        assert_merge_value(true, false, false);
        assert_merge_value(false, true, true);

        assert_merge_value("my_val", true, true);
        assert_merge_value(true, "my_val", "my_val");

        assert_merge_value(1, 2, 2);
    }

    #[test]
    fn merge_event_combines_values_accordingly() {
        // Specify the fields that will be merged.
        // Only the ones listed will be merged from the `incoming` event
        // to the `current`.
        let fields_to_merge = vec![
            "merge".to_string(),
            "merge_a".to_string(),
            "merge_b".to_string(),
            "merge_c".to_string(),
        ];

        let current = {
            let mut log = LogEvent::default();

            log.insert("merge", "hello "); // will be concatenated with the `merged` from `incoming`.
            log.insert("do_not_merge", "my_first_value"); // will remain as is, since it's not selected for merging.

            log.insert("merge_a", true); // will be overwritten with the `merge_a` from `incoming` (since it's a non-bytes kind).
            log.insert("merge_b", 123i64); // will be overwritten with the `merge_b` from `incoming` (since it's a non-bytes kind).

            log.insert("a", true); // will remain as is since it's not selected for merge.
            log.insert("b", 123i64); // will remain as is since it's not selected for merge.

            // `c` is not present in the `current`, and not selected for merge,
            // so it won't be included in the final event.

            log
        };

        let incoming = {
            let mut log = LogEvent::default();

            log.insert("merge", "world"); // will be concatenated to the `merge` from `current`.
            log.insert("do_not_merge", "my_second_value"); // will be ignored, since it's not selected for merge.

            log.insert("merge_b", 456i64); // will be merged in as `456`.
            log.insert("merge_c", false); // will be merged in as `false`.

            // `a` will remain as-is, since it's not marked for merge and
            // neither is it specified in the `incoming` event.
            log.insert("b", 456i64); // `b` not marked for merge, will not change.
            log.insert("c", true); // `c` not marked for merge, will be ignored.

            log
        };

        let mut merged = current;
        merged.merge(incoming, &fields_to_merge);

        let expected = {
            let mut log = LogEvent::default();
            log.insert("merge", "hello world");
            log.insert("do_not_merge", "my_first_value");
            log.insert("a", true);
            log.insert("b", 123i64);
            log.insert("merge_a", true);
            log.insert("merge_b", 456i64);
            log.insert("merge_c", false);
            log
        };

        vector_common::assert_event_data_eq!(merged, expected);
    }

    #[test]
    fn event_fields_iter() {
        let mut log = LogEvent::default();
        log.insert("a", 0);
        log.insert("a.b", 1);
        log.insert("c", 2);
        let actual: Vec<(KeyString, Value)> = log
            .all_event_fields()
            .unwrap()
            .map(|(s, v)| (s, v.clone()))
            .collect();
        assert_eq!(
            actual,
            vec![("a.b".into(), 1.into()), ("c".into(), 2.into())]
        );
    }

    #[test]
    fn metadata_fields_iter() {
        let mut log = LogEvent::default();
        log.insert("%a", 0);
        log.insert("%a.b", 1);
        log.insert("%c", 2);
        let actual: Vec<(KeyString, Value)> = log
            .all_metadata_fields()
            .unwrap()
            .map(|(s, v)| (s, v.clone()))
            .collect();
        assert_eq!(
            actual,
            vec![("%a.b".into(), 1.into()), ("%c".into(), 2.into())]
        );
    }

    #[test]
    fn skip_array_elements() {
        let log = LogEvent::from(Value::from(btreemap! {
            "arr" => [1],
            "obj" => btreemap! {
                "arr" => [1,2,3]
            },
        }));

        let actual: Vec<(KeyString, Value)> = log
            .all_event_fields_skip_array_elements()
            .unwrap()
            .map(|(s, v)| (s, v.clone()))
            .collect();
        assert_eq!(
            actual,
            vec![
                ("arr".into(), [1].into()),
                ("obj.arr".into(), [1, 2, 3].into())
            ]
        );
    }

    #[test]
    fn metadata_set_unique_uuid_v7_source_event_id() {
        // Check if event id is UUID v7
        let log1 = LogEvent::default();
        assert_eq!(
            log1.metadata()
                .source_event_id()
                .expect("source_event_id should be auto-generated for new events")
                .get_version(),
            Some(Version::SortRand)
        );

        // Check if event id is unique on creation
        let log2 = LogEvent::default();
        assert_ne!(
            log1.metadata().source_event_id(),
            log2.metadata().source_event_id()
        );
    }
}
