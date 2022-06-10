use core::fmt;
use std::collections::btree_map;

use serde::{Deserialize, Serialize};
use vector_common::byte_size_of::ByteSizeOf;

use super::{write_list, write_word, MetricTags};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize)]
pub struct MetricSeries {
    #[serde(flatten)]
    pub name: MetricName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<MetricTags>,
}

impl MetricSeries {
    /// Gets a reference to the name of the series.
    pub fn name(&self) -> &MetricName {
        &self.name
    }

    /// Gets a mutable reference to the name of the series.
    pub fn name_mut(&mut self) -> &mut MetricName {
        &mut self.name
    }

    /// Gets an optional reference to the tags of the series.
    pub fn tags(&self) -> Option<&MetricTags> {
        self.tags.as_ref()
    }

    /// Gets an optional mutable reference to the tags of the series.
    pub fn tags_mut(&mut self) -> &mut Option<MetricTags> {
        &mut self.tags
    }

    /// Sets or updates the string value of a tag.
    ///
    /// *Note:* This will create the tags map if it is not present.
    pub fn insert_tag(&mut self, key: String, value: String) -> Option<String> {
        (self.tags.get_or_insert_with(Default::default)).insert(key, value)
    }

    /// Removes the tag entry for the named key, if it exists, and returns the old value.
    ///
    /// *Note:* This will drop the tags map if the tag was the last entry in it.
    pub fn remove_tag(&mut self, key: &str) -> Option<String> {
        match &mut self.tags {
            None => None,
            Some(tags) => {
                let result = tags.remove(key);
                if tags.is_empty() {
                    self.tags = None;
                }
                result
            }
        }
    }

    /// Get the tag entry for the named key. *Note:* This will create
    /// the tags map if it is not present, even if nothing is later
    /// inserted.
    pub fn tag_entry(&mut self, key: String) -> btree_map::Entry<String, String> {
        self.tags.get_or_insert_with(Default::default).entry(key)
    }
}

impl ByteSizeOf for MetricSeries {
    fn allocated_bytes(&self) -> usize {
        self.name.allocated_bytes() + self.tags.allocated_bytes()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize)]
pub struct MetricName {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

impl MetricName {
    /// Gets a reference to the name component of this name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Gets a mutable reference to the name component of this name.
    pub fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    /// Gets a reference to the namespace component of this name.
    pub fn namespace(&self) -> Option<&String> {
        self.namespace.as_ref()
    }

    /// Gets a mutable reference to the namespace component of this name.
    pub fn namespace_mut(&mut self) -> &mut Option<String> {
        &mut self.namespace
    }
}

impl fmt::Display for MetricSeries {
    /// Display a metric series name using something like Prometheus' text format:
    ///
    /// ```text
    /// NAMESPACE_NAME{TAGS}
    /// ```
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(namespace) = &self.name.namespace {
            write_word(fmt, namespace)?;
            write!(fmt, "_")?;
        }
        write_word(fmt, &self.name.name)?;
        write!(fmt, "{{")?;
        if let Some(tags) = &self.tags {
            write_list(fmt, ",", tags.iter(), |fmt, (tag, value)| {
                write_word(fmt, tag).and_then(|()| write!(fmt, "={:?}", value))
            })?;
        }
        write!(fmt, "}}")
    }
}

impl ByteSizeOf for MetricName {
    fn allocated_bytes(&self) -> usize {
        self.name.allocated_bytes() + self.namespace.allocated_bytes()
    }
}
