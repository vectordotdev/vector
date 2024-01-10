use std::{
    fmt,
    num::{NonZeroU64, NonZeroUsize},
    path::{Path, PathBuf},
    slice,
};

use serde::{de, Deserialize, Deserializer, Serialize};
use snafu::{ResultExt, Snafu};
use tracing::Span;
use vector_common::{config::ComponentKey, finalization::Finalizable};
use vector_config::configurable_component;

use crate::{
    topology::{
        builder::{TopologyBuilder, TopologyError},
        channel::{BufferReceiver, BufferSender},
    },
    variants::{DiskV2Buffer, MemoryBuffer},
    Bufferable, WhenFull,
};

#[derive(Debug, Snafu)]
pub enum BufferBuildError {
    #[snafu(display("the configured buffer type requires `data_dir` be specified"))]
    RequiresDataDir,
    #[snafu(display("error occurred when building buffer: {}", source))]
    FailedToBuildTopology { source: TopologyError },
    #[snafu(display("`max_events` must be greater than zero"))]
    InvalidMaxEvents,
}

#[derive(Deserialize, Serialize)]
enum BufferTypeKind {
    #[serde(rename = "memory")]
    Memory,
    #[serde(rename = "disk")]
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
        let mut max_events: Option<NonZeroUsize> = None;
        let mut max_size: Option<NonZeroU64> = None;
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
        let kind = kind.unwrap_or(BufferTypeKind::Memory);
        let when_full = when_full.unwrap_or_default();
        match kind {
            BufferTypeKind::Memory => {
                if max_size.is_some() {
                    return Err(de::Error::unknown_field(
                        "max_size",
                        &["type", "max_events", "when_full"],
                    ));
                }
                Ok(BufferType::Memory {
                    max_events: max_events.unwrap_or_else(memory_buffer_default_max_events),
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

pub const fn memory_buffer_default_max_events() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(500) }
}

/// Disk usage configuration for disk-backed buffers.
#[derive(Debug)]
pub struct DiskUsage {
    id: ComponentKey,
    data_dir: PathBuf,
    max_size: NonZeroU64,
}

impl DiskUsage {
    /// Creates a new `DiskUsage` with the given usage configuration.
    pub fn new(id: ComponentKey, data_dir: PathBuf, max_size: NonZeroU64) -> Self {
        Self {
            id,
            data_dir,
            max_size,
        }
    }

    /// Gets the component key for the component this buffer is attached to.
    pub fn id(&self) -> &ComponentKey {
        &self.id
    }

    /// Gets the maximum size, in bytes, that this buffer can consume on disk.
    pub fn max_size(&self) -> u64 {
        self.max_size.get()
    }

    /// Gets the data directory path that this buffer will store its files on disk.
    pub fn data_dir(&self) -> &Path {
        self.data_dir.as_path()
    }
}

/// A specific type of buffer stage.
#[configurable_component(no_deser)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "type")]
#[configurable(metadata(docs::enum_tag_description = "The type of buffer to use."))]
pub enum BufferType {
    /// A buffer stage backed by an in-memory channel provided by `tokio`.
    ///
    /// This is more performant, but less durable. Data will be lost if Vector is restarted
    /// forcefully or crashes.
    #[configurable(title = "Events are buffered in memory.")]
    #[serde(rename = "memory")]
    Memory {
        /// The maximum number of events allowed in the buffer.
        #[serde(default = "memory_buffer_default_max_events")]
        max_events: NonZeroUsize,

        #[configurable(derived)]
        #[serde(default)]
        when_full: WhenFull,
    },

    /// A buffer stage backed by disk.
    ///
    /// This is less performant, but more durable. Data that has been synchronized to disk will not
    /// be lost if Vector is restarted forcefully or crashes.
    ///
    /// Data is synchronized to disk every 500ms.
    #[configurable(title = "Events are buffered on disk.")]
    #[serde(rename = "disk")]
    DiskV2 {
        /// The maximum size of the buffer on disk.
        ///
        /// Must be at least ~256 megabytes (268435488 bytes).
        #[configurable(
            validation(range(min = 268435488)),
            metadata(docs::type_unit = "bytes")
        )]
        max_size: NonZeroU64,

        #[configurable(derived)]
        #[serde(default)]
        when_full: WhenFull,
    },
}

impl BufferType {
    /// Gets the metadata around disk usage by the buffer, if supported.
    ///
    /// For buffer types that write to disk, `Some(value)` is returned with their usage metadata,
    /// such as maximum size and data directory path.
    ///
    /// Otherwise, `None` is returned.
    pub fn disk_usage(
        &self,
        global_data_dir: Option<PathBuf>,
        id: &ComponentKey,
    ) -> Option<DiskUsage> {
        // All disk-backed buffers require the global data directory to be specified, and
        // non-disk-backed buffers do not require it to be set... so if it's not set here, we ignore
        // it because either:
        // - it's a non-disk-backed buffer, in which case we can just ignore, or
        // - this method is being called at a point before we actually check that a global data
        //   directory is specified because we have a disk buffer present
        //
        // Since we're not able to emit/surface errors about a lack of a global data directory from
        // where this method is called, we simply return `None` to let it reach the code that _does_
        // emit/surface those errors... and once those errors are fixed, this code can return valid
        // disk usage information, which will then be validated and emit any errors for _that_
        // aspect.
        match global_data_dir {
            None => None,
            Some(global_data_dir) => match self {
                Self::Memory { .. } => None,
                Self::DiskV2 { max_size, .. } => {
                    let data_dir = crate::variants::disk_v2::get_disk_v2_data_dir_path(
                        &global_data_dir,
                        id.id(),
                    );

                    Some(DiskUsage::new(id.clone(), data_dir, *max_size))
                }
            },
        }
    }

    /// Adds this buffer type as a stage to an existing [`TopologyBuilder`].
    ///
    /// # Errors
    ///
    /// If a required parameter is missing, or if there is an error building the topology itself, an
    /// error variant will be returned describing the error
    pub fn add_to_builder<T>(
        &self,
        builder: &mut TopologyBuilder<T>,
        data_dir: Option<PathBuf>,
        id: String,
    ) -> Result<(), BufferBuildError>
    where
        T: Bufferable + Clone + Finalizable,
    {
        match *self {
            BufferType::Memory {
                when_full,
                max_events,
            } => {
                builder.stage(MemoryBuffer::new(max_events), when_full);
            }
            BufferType::DiskV2 {
                when_full,
                max_size,
            } => {
                let data_dir = data_dir.ok_or(BufferBuildError::RequiresDataDir)?;
                builder.stage(DiskV2Buffer::new(id, data_dir, max_size), when_full);
            }
        };

        Ok(())
    }
}

/// Buffer configuration.
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
///
// TODO: We need to limit chained buffers to only allowing a single copy of each buffer type to be
// defined, otherwise, for example, two instances of the same disk buffer type in a single chained
// buffer topology would try to both open the same buffer files on disk, which wouldn't work or
// would go horribly wrong.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
#[configurable(
    title = "Configures the buffering behavior for this sink.",
    description = r#"More information about the individual buffer types, and buffer behavior, can be found in the
[Buffering Model][buffering_model] section.

[buffering_model]: /docs/about/under-the-hood/architecture/buffering-model/"#
)]
pub enum BufferConfig {
    /// A single stage buffer topology.
    Single(BufferType),

    /// A chained buffer topology.
    Chained(Vec<BufferType>),
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self::Single(BufferType::Memory {
            max_events: memory_buffer_default_max_events(),
            when_full: WhenFull::default(),
        })
    }
}

impl BufferConfig {
    /// Gets all of the configured stages for this buffer.
    pub fn stages(&self) -> &[BufferType] {
        match self {
            Self::Single(stage) => slice::from_ref(stage),
            Self::Chained(stages) => stages.as_slice(),
        }
    }

    /// Builds the buffer components represented by this configuration.
    ///
    /// The caller gets back a `Sink` and `Stream` implementation that represent a way to push items
    /// into the buffer, as well as pop items out of the buffer, respectively.
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
    ) -> Result<(BufferSender<T>, BufferReceiver<T>), BufferBuildError>
    where
        T: Bufferable + Clone + Finalizable,
    {
        let mut builder = TopologyBuilder::default();

        for stage in self.stages() {
            stage.add_to_builder(&mut builder, data_dir.clone(), buffer_id.clone())?;
        }

        builder
            .build(buffer_id, span)
            .await
            .context(FailedToBuildTopologySnafu)
    }
}

#[cfg(test)]
mod test {
    use std::num::{NonZeroU64, NonZeroUsize};

    use crate::{BufferConfig, BufferType, WhenFull};

    fn check_single_stage(source: &str, expected: BufferType) {
        let config: BufferConfig = serde_yaml::from_str(source).unwrap();
        assert_eq!(config.stages().len(), 1);
        let actual = config.stages().first().unwrap();
        assert_eq!(actual, &expected);
    }

    fn check_multiple_stages(source: &str, expected_stages: &[BufferType]) {
        let config: BufferConfig = serde_yaml::from_str(source).unwrap();
        assert_eq!(config.stages().len(), expected_stages.len());
        for (actual, expected) in config.stages().iter().zip(expected_stages) {
            assert_eq!(actual, expected);
        }
    }

    const BUFFER_CONFIG_NO_MATCH_ERR: &str =
        "data did not match any variant of untagged enum BufferConfig";

    #[test]
    fn parse_empty() {
        let source = "";
        let error = serde_yaml::from_str::<BufferConfig>(source).unwrap_err();
        assert_eq!(error.to_string(), BUFFER_CONFIG_NO_MATCH_ERR);
    }

    #[test]
    fn parse_only_invalid_keys() {
        let source = "foo: 314";
        let error = serde_yaml::from_str::<BufferConfig>(source).unwrap_err();
        assert_eq!(error.to_string(), BUFFER_CONFIG_NO_MATCH_ERR);
    }

    #[test]
    fn parse_partial_invalid_keys() {
        let source = r"max_size: 100
max_events: 42
";
        let error = serde_yaml::from_str::<BufferConfig>(source).unwrap_err();
        assert_eq!(error.to_string(), BUFFER_CONFIG_NO_MATCH_ERR);
    }

    #[test]
    fn parse_without_type_tag() {
        check_single_stage(
            r"
          max_events: 100
          ",
            BufferType::Memory {
                max_events: NonZeroUsize::new(100).unwrap(),
                when_full: WhenFull::Block,
            },
        );
    }

    #[test]
    fn parse_multiple_stages() {
        check_multiple_stages(
            r"
          - max_events: 42
          - max_events: 100
            when_full: drop_newest
          ",
            &[
                BufferType::Memory {
                    max_events: NonZeroUsize::new(42).unwrap(),
                    when_full: WhenFull::Block,
                },
                BufferType::Memory {
                    max_events: NonZeroUsize::new(100).unwrap(),
                    when_full: WhenFull::DropNewest,
                },
            ],
        );
    }

    #[test]
    fn ensure_field_defaults_for_all_types() {
        check_single_stage(
            r"
          type: memory
          ",
            BufferType::Memory {
                max_events: NonZeroUsize::new(500).unwrap(),
                when_full: WhenFull::Block,
            },
        );

        check_single_stage(
            r"
          type: memory
          max_events: 100
          ",
            BufferType::Memory {
                max_events: NonZeroUsize::new(100).unwrap(),
                when_full: WhenFull::Block,
            },
        );

        check_single_stage(
            r"
          type: memory
          when_full: drop_newest
          ",
            BufferType::Memory {
                max_events: NonZeroUsize::new(500).unwrap(),
                when_full: WhenFull::DropNewest,
            },
        );

        check_single_stage(
            r"
          type: memory
          when_full: overflow
          ",
            BufferType::Memory {
                max_events: NonZeroUsize::new(500).unwrap(),
                when_full: WhenFull::Overflow,
            },
        );

        check_single_stage(
            r"
          type: disk
          max_size: 1024
          ",
            BufferType::DiskV2 {
                max_size: NonZeroU64::new(1024).unwrap(),
                when_full: WhenFull::Block,
            },
        );
    }
}
