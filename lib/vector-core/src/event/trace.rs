use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use vector_buffers::EventCount;
use vector_common::EventDataEq;

use super::{
    util, BatchNotifier, EventFinalizer, EventFinalizers, EventMetadata, Finalizable, LogEvent,
    Value,
};
use crate::ByteSizeOf;

/// Traces are a newtype of `LogEvent`
#[derive(Clone, Debug, Default, Deserialize, PartialEq, PartialOrd, Serialize)]
pub struct TraceEvent(LogEvent);

impl TraceEvent {
    pub fn into_parts(self) -> (BTreeMap<String, Value>, EventMetadata) {
        self.0.into_parts()
    }

    pub fn from_parts(fields: BTreeMap<String, Value>, metadata: EventMetadata) -> Self {
        Self(LogEvent::from_parts(fields, metadata))
    }

    pub fn metadata(&self) -> &EventMetadata {
        self.0.metadata()
    }

    pub fn metadata_mut(&mut self) -> &mut EventMetadata {
        self.0.metadata_mut()
    }

    pub fn add_finalizer(&mut self, finalizer: EventFinalizer) {
        self.0.add_finalizer(finalizer);
    }

    pub fn with_batch_notifier(self, batch: &Arc<BatchNotifier>) -> Self {
        Self(self.0.with_batch_notifier(batch))
    }

    pub fn with_batch_notifier_option(self, batch: &Option<Arc<BatchNotifier>>) -> Self {
        Self(self.0.with_batch_notifier_option(batch))
    }

    pub fn as_map(&self) -> &BTreeMap<String, Value> {
        self.0.as_map()
    }

    pub fn get(&self, key: impl AsRef<str>) -> Option<&Value> {
        self.0.get(key.as_ref())
    }

    pub fn get_flat(&self, key: impl AsRef<str>) -> Option<&Value> {
        self.0.as_map().get(key.as_ref())
    }

    pub fn get_mut(&mut self, key: impl AsRef<str>) -> Option<&mut Value> {
        self.0.get_mut(key.as_ref())
    }

    pub fn contains(&self, key: impl AsRef<str>) -> bool {
        util::log::contains(self.0.as_map(), key.as_ref())
    }

    pub fn insert(
        &mut self,
        key: impl AsRef<str>,
        value: impl Into<Value> + Debug,
    ) -> Option<Value> {
        util::log::insert(self.0.as_map_mut(), key.as_ref(), value.into())
    }
}

impl From<LogEvent> for TraceEvent {
    fn from(log: LogEvent) -> Self {
        Self(log)
    }
}

impl From<BTreeMap<String, Value>> for TraceEvent {
    fn from(map: BTreeMap<String, Value>) -> Self {
        Self(map.into())
    }
}

impl ByteSizeOf for TraceEvent {
    fn allocated_bytes(&self) -> usize {
        self.0.allocated_bytes()
    }
}

impl EventCount for TraceEvent {
    fn event_count(&self) -> usize {
        1
    }
}

impl EventDataEq for TraceEvent {
    fn event_data_eq(&self, other: &Self) -> bool {
        self.0.event_data_eq(&other.0)
    }
}

impl Finalizable for TraceEvent {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.0.take_finalizers()
    }
}

impl AsRef<LogEvent> for TraceEvent {
    fn as_ref(&self) -> &LogEvent {
        &self.0
    }
}

impl AsMut<LogEvent> for TraceEvent {
    fn as_mut(&mut self) -> &mut LogEvent {
        &mut self.0
    }
}
