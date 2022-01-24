use std::{fmt, path::PathBuf};

use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};
use snafu::{ResultExt, Snafu};
use tracing::Span;

use crate::{
    topology::{
        builder::{TopologyBuilder, TopologyError},
        channel::{BufferReceiver, BufferSender},
    },
    variant::{DiskV1Buffer, DiskV2Buffer, MemoryV1Buffer, MemoryV2Buffer},
    Acker, Bufferable, WhenFull,
};

#[derive(Debug, Snafu)]
pub enum BufferBuildError {
    #[snafu(display("the configured buffer type requires `data_dir` be specified"))]
    RequiresDataDir,
    #[snafu(display("error occurred when building buffer: {}", source))]
    FailedToBuildTopology { source: TopologyError },
}

#[derive(Deserialize, Serialize)]
enum BufferTypeKind {
    #[serde(rename = "memory")]
    MemoryV1,
    #[serde(rename = "memory_v2")]
    MemoryV2,
    #[serde(rename = "disk")]
    DiskV1,
    #[serde(rename = "disk_v2")]
    DiskV2,
}

const ALL_FIELDS: [&str; 4] = ["type", "max_events", "max_size", "when_full"];

struct BufferTypeVisitor;

impl BufferTypeVisitor {
    fn visit_map_impl<'de, A>(mut map: A) -> Result<BufferType, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        let mut kind: Option<BufferTypeKind> = None;
        let mut max_events: Option<usize> = None;
        let mut max_size: Option<u64> = None;
        let mut when_full: Option<WhenFull> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "type" => {
                    if kind.is_some() {
                        return Err(de::Error::duplicate_field("type"));
                    }
                    kind = Some(map.next_value()?);
                }
                "max_events" => {
                    if max_events.is_some() {
                        return Err(de::Error::duplicate_field("max_events"));
                    }
                    max_events = Some(map.next_value()?);
                }
                "max_size" => {
                    if max_size.is_some() {
                        return Err(de::Error::duplicate_field("max_size"));
                    }
                    max_size = Some(map.next_value()?);
                }
                "when_full" => {
                    if when_full.is_some() {
                        return Err(de::Error::duplicate_field("when_full"));
                    }
                    when_full = Some(map.next_value()?);
                }
                other => {
                    return Err(de::Error::unknown_field(other, &ALL_FIELDS));
                }
            }
        }
        let kind = kind.unwrap_or(BufferTypeKind::MemoryV1);
        let when_full = when_full.unwrap_or_default();
        match kind {
            BufferTypeKind::MemoryV1 => {
                if max_size.is_some() {
                    return Err(de::Error::unknown_field(
                        "max_size",
                        &["type", "max_events", "when_full"],
                    ));
                }
                Ok(BufferType::MemoryV1 {
                    max_events: max_events.unwrap_or_else(memory_buffer_default_max_events),
                    when_full,
                })
            }
            BufferTypeKind::MemoryV2 => {
                if max_size.is_some() {
                    return Err(de::Error::unknown_field(
                        "max_size",
                        &["type", "max_events", "when_full"],
                    ));
                }
                Ok(BufferType::MemoryV2 {
                    max_events: max_events.unwrap_or_else(memory_buffer_default_max_events),
                    when_full,
                })
            }
            BufferTypeKind::DiskV1 => {
                if max_events.is_some() {
                    return Err(de::Error::unknown_field(
                        "max_events",
                        &["type", "max_size", "when_full"],
                    ));
                }
                Ok(BufferType::DiskV1 {
                    max_size: max_size.ok_or_else(|| de::Error::missing_field("max_size"))?,
                    when_full,
                })
            }
            BufferTypeKind::DiskV2 => {
                if max_events.is_some() {
                    return Err(de::Error::unknown_field(
                        "max_events",
                        &["type", "max_size", "when_full"],
                    ));
                }
                Ok(BufferType::DiskV2 {
                    max_size: max_size.ok_or_else(|| de::Error::missing_field("max_size"))?,
                    when_full,
                })
            }
        }
    }
}

impl<'de> de::Visitor<'de> for BufferTypeVisitor {
    type Value = BufferType;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("enum BufferType")
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        BufferTypeVisitor::visit_map_impl(map)
    }
}

impl<'de> Deserialize<'de> for BufferType {
    fn deserialize<D>(deserializer: D) -> Result<BufferType, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(BufferTypeVisitor)
    }
}

struct BufferConfigVisitor;

impl<'de> de::Visitor<'de> for BufferConfigVisitor {
    type Value = BufferConfig;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("enum BufferType")
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        let stage = BufferTypeVisitor::visit_map_impl(map)?;
        Ok(BufferConfig {
            stages: vec![stage],
        })
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut stages = Vec::new();
        while let Some(stage) = seq.next_element()? {
            stages.push(stage);
        }
        Ok(BufferConfig { stages })
    }
}

impl<'de> Deserialize<'de> for BufferConfig {
    fn deserialize<D>(deserializer: D) -> Result<BufferConfig, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(BufferConfigVisitor)
    }
}

impl Serialize for BufferConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.stages.len() {
            0 => Err(ser::Error::custom(
                "buffer config cannot be empty when serializing",
            )),
            1 => self.stages.first().unwrap().serialize(serializer),
            _ => self.stages.serialize(serializer),
        }
    }
}

pub const fn memory_buffer_default_max_events() -> usize {
    500
}

/// A specific type of buffer stage.
#[derive(Copy, Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum BufferType {
    /// A buffer stage backed by an in-memory channel provided by `futures`.
    #[serde(rename = "memory")]
    MemoryV1 {
        #[serde(default = "memory_buffer_default_max_events")]
        max_events: usize,
        #[serde(default)]
        when_full: WhenFull,
    },
    /// A buffer stage backed by an in-memory channel provided by `tokio`.
    #[serde(rename = "memory_v2")]
    MemoryV2 {
        #[serde(default = "memory_buffer_default_max_events")]
        max_events: usize,
        #[serde(default)]
        when_full: WhenFull,
    },
    /// A buffer stage backed by an on-disk database, powered by LevelDB.
    #[serde(rename = "disk")]
    DiskV1 {
        max_size: u64,
        #[serde(default)]
        when_full: WhenFull,
    },
    /// A buffer stage backed by disk.
    #[serde(rename = "disk_v2")]
    DiskV2 {
        max_size: u64,
        #[serde(default)]
        when_full: WhenFull,
    },
}

impl BufferType {
    /// Adds this buffer type as a stage to an existing [`TopologyBuilder`].
    ///
    /// # Errors
    ///
    /// If a required parameter is missing, or if there is an error building the topology itself, an
    /// error variant will be returned desribing the error
    pub fn add_to_builder<T>(
        &self,
        builder: &mut TopologyBuilder<T>,
        data_dir: Option<PathBuf>,
        id: String,
    ) -> Result<(), BufferBuildError>
    where
        T: Bufferable + Clone,
    {
        match *self {
            BufferType::MemoryV1 {
                when_full,
                max_events,
            } => {
                builder.stage(MemoryV1Buffer::new(max_events), when_full);
            }
            BufferType::MemoryV2 {
                when_full,
                max_events,
            } => {
                builder.stage(MemoryV2Buffer::new(max_events), when_full);
            }
            BufferType::DiskV1 {
                when_full,
                max_size,
            } => {
                let data_dir = data_dir.ok_or(BufferBuildError::RequiresDataDir)?;
                builder.stage(DiskV1Buffer::new(id, data_dir, max_size), when_full);
            }
            BufferType::DiskV2 {
                when_full,
                max_size,
            } => {
                warn!("!!!! The `disk_v2` buffer type is not yet stable.  Data loss may be encountered. !!!!");
                let data_dir = data_dir.ok_or(BufferBuildError::RequiresDataDir)?;
                builder.stage(DiskV2Buffer::new(id, data_dir, max_size), when_full);
            }
        };

        Ok(())
    }
}

/// A buffer configuration.
///
/// Buffers are compromised of stages(*) that form a buffer _topology_, with input items being
/// subject to configurable behavior when each stage reaches configured limits.  Buffers are
/// configured for sinks, where backpressure from the sink can be handled by the buffer.  This
/// allows absorbing temporary load, or potentially adding write-ahead-log behavior to a sink to
/// increase the durability of a given Vector pipeline.
///
/// While we use the term "buffer topology" here, a buffer topology is referred to by the more
/// common "buffer" or "buffers" shorthand.  This is related to buffers originally being a single
/// component, where you could only choose which buffer type to use.  As we expand buffer
/// functionality to allow chaining buffers together, you'll see "buffer topology" used in internal
/// documentation to correctly reflect the internal structure.
#[derive(Clone, Debug, PartialEq)]
pub struct BufferConfig {
    pub stages: Vec<BufferType>,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            stages: vec![BufferType::MemoryV1 {
                max_events: memory_buffer_default_max_events(),
                when_full: WhenFull::default(),
            }],
        }
    }
}

impl BufferConfig {
    /// Gets all of the configured stages for this buffer.
    pub fn stages(&self) -> &[BufferType] {
        &self.stages
    }

    /// Builds the buffer components represented by this configuration.
    ///
    /// The caller gets back a `Sink` and `Stream` implementation that represent a way to push items
    /// into the buffer, as well as pop items out of the buffer, respectively.  The `Acker` is
    /// provided to callers in order to update the buffer when popped items have been processed and
    /// can be dropped or deleted, depending on the underlying buffer implementation.
    ///
    /// # Errors
    ///
    /// If the buffer is configured with anything other than a single stage, an error variant will
    /// be thrown.
    ///
    /// If a disk buffer stage is configured and the data directory provided is `None`, an error
    /// variant will be thrown.
    #[allow(clippy::needless_pass_by_value)]
    pub async fn build<T>(
        &self,
        data_dir: Option<PathBuf>,
        buffer_id: String,
        span: Span,
    ) -> Result<(BufferSender<T>, BufferReceiver<T>, Acker), BufferBuildError>
    where
        T: Bufferable + Clone,
    {
        let mut builder = TopologyBuilder::default();

        for stage in self.stages.iter().copied() {
            stage.add_to_builder(&mut builder, data_dir.clone(), buffer_id.clone())?;
        }

        builder
            .build(span)
            .await
            .context(FailedToBuildTopologySnafu)
    }
}

#[cfg(test)]
mod test {
    use crate::{BufferConfig, BufferType, WhenFull};

    fn check_single_stage(source: &str, expected: BufferType) {
        let config: BufferConfig = serde_yaml::from_str(source).unwrap();
        assert_eq!(config.stages.len(), 1);
        let actual = config.stages.first().unwrap();
        assert_eq!(actual, &expected);
    }

    fn check_multiple_stages(source: &str, expected_stages: &[BufferType]) {
        let config: BufferConfig = serde_yaml::from_str(source).unwrap();
        assert_eq!(config.stages.len(), expected_stages.len());
        for (actual, expected) in config.stages().iter().zip(expected_stages) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn parse_empty() {
        let source = "";
        let error = serde_yaml::from_str::<BufferConfig>(source).unwrap_err();
        assert_eq!(error.to_string(), "EOF while parsing a value");
    }

    #[test]
    fn parse_only_invalid_keys() {
        let source = "foo: 314";
        let error = serde_yaml::from_str::<BufferConfig>(source).unwrap_err();
        assert_eq!(
            error.to_string(),
            "unknown field `foo`, expected one of `type`, `max_events`, `max_size`, `when_full` at line 1 column 4"
        );
    }

    #[test]
    fn parse_partial_invalid_keys() {
        let source = r#"max_size: 100
max_events: 42
"#;
        let error = serde_yaml::from_str::<BufferConfig>(source).unwrap_err();
        assert_eq!(
            error.to_string(),
            "unknown field `max_size`, expected one of `type`, `max_events`, `when_full` at line 1 column 9"
        );
    }

    #[test]
    fn parse_without_type_tag() {
        check_single_stage(
            r#"
          max_events: 100
          "#,
            BufferType::MemoryV1 {
                max_events: 100,
                when_full: WhenFull::Block,
            },
        );
    }

    #[test]
    fn parse_multiple_stages() {
        check_multiple_stages(
            r#"
          - max_events: 42
          - max_events: 100
            when_full: drop_newest
          "#,
            &[
                BufferType::MemoryV1 {
                    max_events: 42,
                    when_full: WhenFull::Block,
                },
                BufferType::MemoryV1 {
                    max_events: 100,
                    when_full: WhenFull::DropNewest,
                },
            ],
        );
    }

    #[test]
    fn ensure_field_defaults_for_all_types() {
        check_single_stage(
            r#"
          type: memory
          "#,
            BufferType::MemoryV1 {
                max_events: 500,
                when_full: WhenFull::Block,
            },
        );

        check_single_stage(
            r#"
          type: memory
          max_events: 100
          "#,
            BufferType::MemoryV1 {
                max_events: 100,
                when_full: WhenFull::Block,
            },
        );

        check_single_stage(
            r#"
          type: memory
          when_full: drop_newest
          "#,
            BufferType::MemoryV1 {
                max_events: 500,
                when_full: WhenFull::DropNewest,
            },
        );

        check_single_stage(
            r#"
          type: memory
          when_full: overflow
          "#,
            BufferType::MemoryV1 {
                max_events: 500,
                when_full: WhenFull::Overflow,
            },
        );

        check_single_stage(
            r#"
          type: disk
          max_size: 1024
          "#,
            BufferType::DiskV1 {
                max_size: 1024,
                when_full: WhenFull::Block,
            },
        );

        check_single_stage(
            r#"
          type: memory_v2
          "#,
            BufferType::MemoryV2 {
                max_events: 500,
                when_full: WhenFull::Block,
            },
        );

        check_single_stage(
            r#"
          type: memory_v2
          max_events: 100
          "#,
            BufferType::MemoryV2 {
                max_events: 100,
                when_full: WhenFull::Block,
            },
        );

        check_single_stage(
            r#"
          type: memory_v2
          when_full: drop_newest
          "#,
            BufferType::MemoryV2 {
                max_events: 500,
                when_full: WhenFull::DropNewest,
            },
        );

        check_single_stage(
            r#"
          type: memory_v2
          when_full: overflow
          "#,
            BufferType::MemoryV2 {
                max_events: 500,
                when_full: WhenFull::Overflow,
            },
        );

        check_single_stage(
            r#"
          type: disk_v2
          max_size: 1024
          "#,
            BufferType::DiskV2 {
                max_size: 1024,
                when_full: WhenFull::Block,
            },
        );
    }
}
