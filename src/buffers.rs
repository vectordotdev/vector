use crate::config::{ComponentKey, Resource};
use crate::event::Event;
use futures::Stream;
use serde::{
    de::{Deserializer, Error, Visitor},
    Deserialize, Serialize,
};
use std::path::PathBuf;
pub use vector_core::buffers::*;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum BufferConfigKind {
    Memory,
    #[cfg(feature = "disk-buffer")]
    Disk,
}

#[cfg(feature = "disk-buffer")]
const ALL_FIELDS: [&str; 4] = ["type", "max_events", "max_size", "when_full"];
#[cfg(not(feature = "disk-buffer"))]
const ALL_FIELDS: [&str; 3] = ["type", "max_events", "when_full"];

struct BufferConfigVisitor;

impl<'de> Visitor<'de> for BufferConfigVisitor {
    type Value = BufferConfig;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("enum BufferConfig")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut kind: Option<BufferConfigKind> = None;
        let mut max_events: Option<usize> = None;
        #[cfg(feature = "disk-buffer")]
        let mut max_size: Option<usize> = None;
        let mut when_full: Option<WhenFull> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "type" => {
                    if kind.is_some() {
                        return Err(Error::duplicate_field("type"));
                    }
                    kind = Some(map.next_value()?);
                }
                "max_events" => {
                    if max_events.is_some() {
                        return Err(Error::duplicate_field("max_events"));
                    }
                    max_events = Some(map.next_value()?);
                }
                #[cfg(feature = "disk-buffer")]
                "max_size" => {
                    if max_size.is_some() {
                        return Err(Error::duplicate_field("max_size"));
                    }
                    max_size = Some(map.next_value()?);
                }
                "when_full" => {
                    if when_full.is_some() {
                        return Err(Error::duplicate_field("when_full"));
                    }
                    when_full = Some(map.next_value()?);
                }
                other => {
                    return Err(Error::unknown_field(other, &ALL_FIELDS));
                }
            }
        }
        let kind = kind.unwrap_or(BufferConfigKind::Memory);
        let when_full = when_full.unwrap_or_default();
        match kind {
            BufferConfigKind::Memory => {
                #[cfg(feature = "disk-buffer")]
                if max_size.is_some() {
                    return Err(Error::unknown_field(
                        "max_size",
                        &["type", "max_events", "when_full"],
                    ));
                }
                Ok(BufferConfig::Memory {
                    max_events: max_events.unwrap_or_else(BufferConfig::memory_max_events),
                    when_full,
                })
            }
            #[cfg(feature = "disk-buffer")]
            BufferConfigKind::Disk => {
                if max_events.is_some() {
                    return Err(Error::unknown_field(
                        "max_events",
                        &["type", "max_size", "when_full"],
                    ));
                }
                Ok(BufferConfig::Disk {
                    max_size: max_size.ok_or_else(|| Error::missing_field("max_size"))?,
                    when_full,
                })
            }
        }
    }
}

impl<'de> Deserialize<'de> for BufferConfig {
    fn deserialize<D>(deserializer: D) -> Result<BufferConfig, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(BufferConfigVisitor)
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum BufferConfig {
    Memory {
        #[serde(default = "BufferConfig::memory_max_events")]
        max_events: usize,
        #[serde(default)]
        when_full: WhenFull,
    },
    #[cfg(feature = "disk-buffer")]
    Disk {
        max_size: usize,
        #[serde(default)]
        when_full: WhenFull,
    },
}

impl Default for BufferConfig {
    fn default() -> Self {
        BufferConfig::Memory {
            max_events: BufferConfig::memory_max_events(),
            when_full: Default::default(),
        }
    }
}

pub(crate) type EventStream = Box<dyn Stream<Item = Event> + Unpin + Send>;

impl BufferConfig {
    #[inline]
    const fn memory_max_events() -> usize {
        500
    }

    #[cfg_attr(not(feature = "disk-buffer"), allow(unused))]
    pub fn build(
        &self,
        data_dir: &Option<PathBuf>,
        sink_id: &ComponentKey,
    ) -> Result<(BufferInputCloner<Event>, EventStream, Acker), String> {
        let variant = match &self {
            BufferConfig::Memory {
                max_events,
                when_full,
            } => Variant::Memory {
                max_events: *max_events,
                when_full: *when_full,
            },
            #[cfg(feature = "disk-buffer")]
            BufferConfig::Disk {
                max_size,
                when_full,
            } => Variant::Disk {
                max_size: *max_size,
                when_full: *when_full,
                data_dir: data_dir
                    .as_ref()
                    .ok_or_else(|| "Must set data_dir to use on-disk buffering.".to_string())?
                    .to_path_buf(),
                id: sink_id.to_string(),
            },
        };
        build(variant)
    }

    /// Resources that the sink is using.
    #[cfg_attr(not(feature = "disk-buffer"), allow(unused))]
    #[allow(clippy::missing_const_for_fn)] // False positive, allocations are not allowed in const fns
    pub fn resources(&self, sink_id: &str) -> Vec<Resource> {
        match self {
            BufferConfig::Memory { .. } => Vec::new(),
            #[cfg(feature = "disk-buffer")]
            BufferConfig::Disk { .. } => vec![Resource::DiskBuffer(sink_id.to_string())],
        }
    }
}

#[cfg(test)]
mod test {
    use crate::buffers::{BufferConfig, WhenFull};

    fn check(source: &str, config: BufferConfig) {
        let conf: BufferConfig = toml::from_str(source).unwrap();
        assert_eq!(toml::to_string(&conf), toml::to_string(&config));
    }

    #[test]
    fn config_default_values() {
        check(
            r#"
          type = "memory"
          "#,
            BufferConfig::Memory {
                max_events: 500,
                when_full: WhenFull::Block,
            },
        );

        check(
            r#"
          type = "memory"
          max_events = 100
          "#,
            BufferConfig::Memory {
                max_events: 100,
                when_full: WhenFull::Block,
            },
        );

        check(
            r#"
          type = "memory"
          when_full = "drop_newest"
          "#,
            BufferConfig::Memory {
                max_events: 500,
                when_full: WhenFull::DropNewest,
            },
        );

        #[cfg(feature = "disk-buffer")]
        check(
            r#"
          type = "disk"
          max_size = 1024
          "#,
            BufferConfig::Disk {
                max_size: 1024,
                when_full: WhenFull::Block,
            },
        );
    }

    #[test]
    fn parse_without_tag() {
        check(
            r#"
          max_events = 100
          "#,
            BufferConfig::Memory {
                max_events: 100,
                when_full: WhenFull::Block,
            },
        );
    }

    #[test]
    fn parse_invalid_keys() {
        let source = r#"
    max_events = 100
    max_size = 42
    "#;
        let error = toml::from_str::<BufferConfig>(source).unwrap_err();
        assert_eq!(
            error.to_string(),
            "unknown field `max_size`, expected one of `type`, `max_events`, `when_full` at line 1 column 1"
        );
    }
}
