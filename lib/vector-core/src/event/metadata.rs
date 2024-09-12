#![deny(missing_docs)]

use std::{borrow::Cow, collections::BTreeMap, fmt, sync::Arc};

use derivative::Derivative;
use lookup::OwnedTargetPath;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vector_common::{byte_size_of::ByteSizeOf, config::ComponentKey, EventDataEq};
use vrl::{
    compiler::SecretTarget,
    value::{KeyString, Kind, Value},
};

use super::{BatchNotifier, EventFinalizer, EventFinalizers, EventStatus, ObjectMap};
use crate::{
    config::{LogNamespace, OutputId},
    schema,
};

const DATADOG_API_KEY: &str = "datadog_api_key";
const SPLUNK_HEC_TOKEN: &str = "splunk_hec_token";

/// The top-level metadata structure contained by both `struct Metric`
/// and `struct LogEvent` types.
#[derive(Clone, Debug, Deserialize, Serialize, Derivative)]
#[derivative(PartialEq)]
pub struct EventMetadata {
    /// Arbitrary data stored with an event
    #[serde(default = "default_metadata_value")]
    pub(crate) value: Value,

    /// Storage for secrets
    #[serde(default)]
    pub(crate) secrets: Secrets,

    #[serde(default, skip)]
    finalizers: EventFinalizers,

    /// The id of the source
    pub(crate) source_id: Option<Arc<ComponentKey>>,

    /// The type of the source
    pub(crate) source_type: Option<Cow<'static, str>>,

    /// The id of the component this event originated from. This is used to
    /// determine which schema definition to attach to an event in transforms.
    /// This should always have a value set for events in transforms. It will always be `None`
    /// in a source, and there is currently no use-case for reading the value in a sink.
    pub(crate) upstream_id: Option<Arc<OutputId>>,

    /// An identifier for a globally registered schema definition which provides information about
    /// the event shape (type information, and semantic meaning of fields).
    /// This definition is only currently valid for logs, and shouldn't be used for other event types.
    ///
    /// TODO(Jean): must not skip serialization to track schemas across restarts.
    #[serde(default = "default_schema_definition", skip)]
    schema_definition: Arc<schema::Definition>,

    /// A store of values that may be dropped during the encoding process but may be needed
    /// later on. The map is indexed by meaning.
    /// Currently this is just used for the `service`. If the service field is dropped by `only_fields`
    /// we need to ensure it is still available later on for emitting metrics tagged by the service.
    /// This field could almost be keyed by `&'static str`, but because it needs to be deserializable
    /// we have to use `String`.
    dropped_fields: ObjectMap,

    /// Metadata to track the origin of metrics. This is always `None` for log and trace events.
    /// Only a small set of Vector sources and transforms explicitly set this field.
    #[serde(default)]
    pub(crate) datadog_origin_metadata: Option<DatadogMetricOriginMetadata>,

    /// An internal vector id that can be used to identify this event across all components.
    #[derivative(PartialEq = "ignore")]
    pub(crate) source_event_id: Option<Uuid>,
}

/// Metric Origin metadata for submission to Datadog.
#[derive(Clone, Default, Debug, Deserialize, PartialEq, Serialize)]
pub struct DatadogMetricOriginMetadata {
    /// `OriginProduct`
    origin_product: Option<u32>,
    /// `OriginCategory`
    origin_category: Option<u32>,
    /// `OriginService`
    origin_service: Option<u32>,
}

impl DatadogMetricOriginMetadata {
    /// Creates a new `DatadogMetricOriginMetadata`.
    /// When Vector sends out metrics containing the Origin metadata, it should do so with
    /// all of the fields defined.
    /// The edge case where the Origin metadata is created within a component and does not
    /// initially contain all of the metadata fields, is in the `log_to_metric` transform.
    #[must_use]
    pub fn new(product: Option<u32>, category: Option<u32>, service: Option<u32>) -> Self {
        Self {
            origin_product: product,
            origin_category: category,
            origin_service: service,
        }
    }

    /// Returns a reference to the `OriginProduct`.
    pub fn product(&self) -> Option<u32> {
        self.origin_product
    }

    /// Returns a reference to the `OriginCategory`.
    pub fn category(&self) -> Option<u32> {
        self.origin_category
    }

    /// Returns a reference to the `OriginService`.
    pub fn service(&self) -> Option<u32> {
        self.origin_service
    }
}

fn default_metadata_value() -> Value {
    Value::Object(ObjectMap::new())
}

impl EventMetadata {
    /// Creates `EventMetadata` with the given `Value`, and the rest of the fields with default values
    pub fn default_with_value(value: Value) -> Self {
        Self {
            value,
            ..Default::default()
        }
    }

    /// Returns a reference to the metadata value
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Returns a mutable reference to the metadata value
    pub fn value_mut(&mut self) -> &mut Value {
        &mut self.value
    }

    /// Returns a reference to the secrets
    pub fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    /// Returns a mutable reference to the secrets
    pub fn secrets_mut(&mut self) -> &mut Secrets {
        &mut self.secrets
    }

    /// Returns a reference to the metadata source id.
    #[must_use]
    pub fn source_id(&self) -> Option<&Arc<ComponentKey>> {
        self.source_id.as_ref()
    }

    /// Returns a reference to the metadata source type.
    #[must_use]
    pub fn source_type(&self) -> Option<&str> {
        self.source_type.as_deref()
    }

    /// Returns a reference to the metadata parent id. This is the `OutputId`
    /// of the previous component the event was sent through (if any).
    #[must_use]
    pub fn upstream_id(&self) -> Option<&OutputId> {
        self.upstream_id.as_deref()
    }

    /// Sets the `source_id` in the metadata to the provided value.
    pub fn set_source_id(&mut self, source_id: Arc<ComponentKey>) {
        self.source_id = Some(source_id);
    }

    /// Sets the `source_type` in the metadata to the provided value.
    pub fn set_source_type<S: Into<Cow<'static, str>>>(&mut self, source_type: S) {
        self.source_type = Some(source_type.into());
    }

    /// Sets the `upstream_id` in the metadata to the provided value.
    pub fn set_upstream_id(&mut self, upstream_id: Arc<OutputId>) {
        self.upstream_id = Some(upstream_id);
    }

    /// Return the datadog API key, if it exists
    pub fn datadog_api_key(&self) -> Option<Arc<str>> {
        self.secrets.get(DATADOG_API_KEY).cloned()
    }

    /// Set the datadog API key to passed value
    pub fn set_datadog_api_key(&mut self, secret: Arc<str>) {
        self.secrets.insert(DATADOG_API_KEY, secret);
    }

    /// Return the splunk hec token, if it exists
    pub fn splunk_hec_token(&self) -> Option<Arc<str>> {
        self.secrets.get(SPLUNK_HEC_TOKEN).cloned()
    }

    /// Set the splunk hec token to passed value
    pub fn set_splunk_hec_token(&mut self, secret: Arc<str>) {
        self.secrets.insert(SPLUNK_HEC_TOKEN, secret);
    }

    /// Adds the value to the dropped fields list.
    /// There is currently no way to remove a field from this list, so if a field is dropped
    /// and then the field is re-added with a new value - the dropped value will still be
    /// retrieved.
    pub fn add_dropped_field(&mut self, meaning: KeyString, value: Value) {
        self.dropped_fields.insert(meaning, value);
    }

    /// Fetches the dropped field by meaning.
    pub fn dropped_field(&self, meaning: impl AsRef<str>) -> Option<&Value> {
        self.dropped_fields.get(meaning.as_ref())
    }

    /// Returns a reference to the `DatadogMetricOriginMetadata`.
    pub fn datadog_origin_metadata(&self) -> Option<&DatadogMetricOriginMetadata> {
        self.datadog_origin_metadata.as_ref()
    }

    /// Returns a reference to the event id.
    pub fn source_event_id(&self) -> Option<Uuid> {
        self.source_event_id
    }
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self {
            value: Value::Object(ObjectMap::new()),
            secrets: Secrets::new(),
            finalizers: Default::default(),
            schema_definition: default_schema_definition(),
            source_id: None,
            source_type: None,
            upstream_id: None,
            dropped_fields: ObjectMap::new(),
            datadog_origin_metadata: None,
            source_event_id: Some(Uuid::now_v7()),
        }
    }
}

fn default_schema_definition() -> Arc<schema::Definition> {
    Arc::new(schema::Definition::new_with_default_metadata(
        Kind::any(),
        [LogNamespace::Legacy, LogNamespace::Vector],
    ))
}

impl ByteSizeOf for EventMetadata {
    fn allocated_bytes(&self) -> usize {
        // NOTE we don't count the `str` here because it's allocated somewhere
        // else. We're just moving around the pointer, which is already captured
        // by `ByteSizeOf::size_of`.
        self.finalizers.allocated_bytes()
    }
}

impl EventMetadata {
    /// Replaces the existing event finalizers with the given one.
    #[must_use]
    pub fn with_finalizer(mut self, finalizer: EventFinalizer) -> Self {
        self.finalizers = EventFinalizers::new(finalizer);
        self
    }

    /// Replaces the existing event finalizers with the given ones.
    #[must_use]
    pub fn with_finalizers(mut self, finalizers: EventFinalizers) -> Self {
        self.finalizers = finalizers;
        self
    }

    /// Replace the finalizer with a new one created from the given batch notifier.
    #[must_use]
    pub fn with_batch_notifier(self, batch: &BatchNotifier) -> Self {
        self.with_finalizer(EventFinalizer::new(batch.clone()))
    }

    /// Replace the finalizer with a new one created from the given optional batch notifier.
    #[must_use]
    pub fn with_batch_notifier_option(self, batch: &Option<BatchNotifier>) -> Self {
        match batch {
            Some(batch) => self.with_finalizer(EventFinalizer::new(batch.clone())),
            None => self,
        }
    }

    /// Replace the schema definition with the given one.
    #[must_use]
    pub fn with_schema_definition(mut self, schema_definition: &Arc<schema::Definition>) -> Self {
        self.schema_definition = Arc::clone(schema_definition);
        self
    }

    /// Replaces the existing `source_type` with the given one.
    #[must_use]
    pub fn with_source_type<S: Into<Cow<'static, str>>>(mut self, source_type: S) -> Self {
        self.source_type = Some(source_type.into());
        self
    }

    /// Replaces the existing `DatadogMetricOriginMetadata` with the given one.
    #[must_use]
    pub fn with_origin_metadata(mut self, origin_metadata: DatadogMetricOriginMetadata) -> Self {
        self.datadog_origin_metadata = Some(origin_metadata);
        self
    }

    /// Replaces the existing `source_event_id` with the given one.
    #[must_use]
    pub fn with_source_event_id(mut self, source_event_id: Option<Uuid>) -> Self {
        self.source_event_id = source_event_id;
        self
    }

    /// Merge the other `EventMetadata` into this.
    /// If a Datadog API key is not set in `self`, the one from `other` will be used.
    /// If a Splunk HEC token is not set in `self`, the one from `other` will be used.
    pub fn merge(&mut self, other: Self) {
        self.finalizers.merge(other.finalizers);
        self.secrets.merge(other.secrets);

        // Update `source_event_id` if necessary.
        match (self.source_event_id, other.source_event_id) {
            (None, Some(id)) => {
                self.source_event_id = Some(id);
            }
            (Some(uuid1), Some(uuid2)) if uuid2 < uuid1 => {
                self.source_event_id = Some(uuid2);
            }
            _ => {} // Keep the existing value.
        };
    }

    /// Update the finalizer(s) status.
    pub fn update_status(&self, status: EventStatus) {
        self.finalizers.update_status(status);
    }

    /// Update the finalizers' sources.
    pub fn update_sources(&mut self) {
        self.finalizers.update_sources();
    }

    /// Gets a reference to the event finalizers.
    pub fn finalizers(&self) -> &EventFinalizers {
        &self.finalizers
    }

    /// Add a new event finalizer to the existing list of event finalizers.
    pub fn add_finalizer(&mut self, finalizer: EventFinalizer) {
        self.finalizers.add(finalizer);
    }

    /// Consumes all event finalizers and returns them, leaving the list of event finalizers empty.
    pub fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }

    /// Merges the given event finalizers into the existing list of event finalizers.
    pub fn merge_finalizers(&mut self, finalizers: EventFinalizers) {
        self.finalizers.merge(finalizers);
    }

    /// Get the schema definition.
    pub fn schema_definition(&self) -> &Arc<schema::Definition> {
        &self.schema_definition
    }

    /// Set the schema definition.
    pub fn set_schema_definition(&mut self, definition: &Arc<schema::Definition>) {
        self.schema_definition = Arc::clone(definition);
    }

    /// Helper function to add a semantic meaning to the schema definition.
    ///
    /// This replaces the common code sequence of:
    ///
    /// ```ignore
    /// let new_schema = log_event
    ///     .metadata()
    ///     .schema_definition()
    ///     .as_ref()
    ///     .clone()
    ///     .with_meaning(target_path, meaning);
    /// log_event
    ///     .metadata_mut()
    ///     .set_schema_definition(new_schema);
    /// ````
    pub fn add_schema_meaning(&mut self, target_path: OwnedTargetPath, meaning: &str) {
        let schema = Arc::make_mut(&mut self.schema_definition);
        schema.add_meaning(target_path, meaning);
    }
}

impl EventDataEq for EventMetadata {
    fn event_data_eq(&self, _other: &Self) -> bool {
        // Don't compare the metadata, it is not "event data".
        true
    }
}

/// This is a simple wrapper to allow attaching `EventMetadata` to any
/// other type. This is primarily used in conversion functions, such as
/// `impl From<X> for WithMetadata<Y>`.
pub struct WithMetadata<T> {
    /// The data item being wrapped.
    pub data: T,
    /// The additional metadata sidecar.
    pub metadata: EventMetadata,
}

impl<T> WithMetadata<T> {
    /// Convert from one wrapped type to another, where the underlying
    /// type allows direct conversion.
    // We would like to `impl From` instead, but this fails due to
    // conflicting implementations of `impl<T> From<T> for T`.
    pub fn into<T1: From<T>>(self) -> WithMetadata<T1> {
        WithMetadata {
            data: T1::from(self.data),
            metadata: self.metadata,
        }
    }
}

/// A container that holds secrets.
#[derive(Clone, Default, Deserialize, Eq, PartialEq, PartialOrd, Serialize)]
pub struct Secrets(BTreeMap<String, Arc<str>>);

impl fmt::Debug for Secrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();
        for key in self.0.keys() {
            map.entry(key, &"<redacted secret>");
        }
        map.finish()
    }
}

impl Secrets {
    /// Creates a new, empty container.
    #[must_use]
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns `true` if the container contains no secrets.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Gets a secret by its name.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Arc<str>> {
        self.0.get(key)
    }

    /// Inserts a new secret into the container.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Arc<str>>) {
        self.0.insert(key.into(), value.into());
    }

    /// Removes a secret
    pub fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }

    /// Merged both together. If there are collisions, the value from `self` is kept.
    pub fn merge(&mut self, other: Self) {
        for (key, value) in other.0 {
            self.0.entry(key).or_insert(value);
        }
    }
}

impl SecretTarget for Secrets {
    fn get_secret(&self, key: &str) -> Option<&str> {
        self.get(key).map(AsRef::as_ref)
    }

    fn insert_secret(&mut self, key: &str, value: &str) {
        self.insert(key, value);
    }

    fn remove_secret(&mut self, key: &str) {
        self.remove(key);
    }
}

impl IntoIterator for Secrets {
    type Item = (String, Arc<str>);
    type IntoIter = std::collections::btree_map::IntoIter<String, Arc<str>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const SECRET: &str = "secret";
    const SECRET2: &str = "secret2";

    #[test]
    fn metadata_hardcoded_secrets_get_set() {
        let mut metadata = EventMetadata::default();
        metadata.set_datadog_api_key(Arc::from(SECRET));
        metadata.set_splunk_hec_token(Arc::from(SECRET2));
        assert_eq!(metadata.datadog_api_key().unwrap().as_ref(), SECRET);
        assert_eq!(metadata.splunk_hec_token().unwrap().as_ref(), SECRET2);
    }

    #[test]
    fn secrets_merge() {
        let mut a = Secrets::new();
        a.insert("key-a", "value-a1");
        a.insert("key-b", "value-b1");

        let mut b = Secrets::new();
        b.insert("key-b", "value-b2");
        b.insert("key-c", "value-c2");

        a.merge(b);

        assert_eq!(a.get("key-a").unwrap().as_ref(), "value-a1");
        assert_eq!(a.get("key-b").unwrap().as_ref(), "value-b1");
        assert_eq!(a.get("key-c").unwrap().as_ref(), "value-c2");
    }

    #[test]
    fn metadata_source_event_id_merging() {
        let m1 = EventMetadata::default();
        let m2 = EventMetadata::default();

        {
            let mut merged = m1.clone();
            merged.merge(m2.clone());
            assert_eq!(merged.source_event_id, m1.source_event_id);
        }

        {
            let mut merged = m2.clone();
            merged.merge(m1.clone());
            assert_eq!(merged.source_event_id, m1.source_event_id);
        }
    }
}
