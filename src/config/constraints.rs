
#![allow(dead_code)]

use vector_config::constraints::{InstancePath, Computed};
use vector_core::config::DataType;

use crate::codecs::{EncodingConfig, EncodingConfigWithFraming};

// http sink: inputs mapped straight via acceptable inputs for codec
// papertail sink: inputs mapped via acceptable inputs for codec, AND'd against log type, with
//                 schema requirement
// redis sink: inputs mapped via acceptable inputs for codec, AND'd against log type
// socket sink: inputs mapped via acceptable inputs for codec, based on configured mode
//              (self.mode), all of which are AND'd against log type
// statsd sink: fixed input of metrics

/// A codec that can encode a specific set of event data types.
pub trait ConstrainableCodec {
    fn encodeable_data_types(&self) -> DataType;
}

impl ConstrainableCodec for EncodingConfig {
    fn encodeable_data_types(&self) -> DataType {
        self.config().input_type()
    }
}

impl ConstrainableCodec for EncodingConfigWithFraming {
    fn encodeable_data_types(&self) -> DataType {
        self.config().1.input_type()
    }
}

/// A codec constraint that is derived from the codec configuration of the sink.
pub struct DerivedCodecConstraint {
    /// Instance path in the configuration to the codec configuration.
    codec_path: InstancePath,

    /// An optional data type filter that is applied to the derived constraint.
    ///
    /// Used to limit a component to specific event data types, without specifying a fixed data type
    /// that the configured codec may or may not support. In other words, a sink may only support
    /// logs and metrics, but if the configured codec supports logs and traces, then we would only
    /// want to indicate that logs are supported ([logs + traces] AND [logs + metrics] == [logs]).
    filter: Option<DataType>,
}

impl DerivedCodecConstraint {
    /// Configures a data type filter for this constraint.
    ///
    /// When set, the data type derived from the sink's codec will be masked (logical AND) with the
    /// configured filter.
    ///
    /// For example, if the configured codec supports encoding both logs and traces, but the filter
    /// is set to logs and metrics, then the resulting data type would only include logs.
    pub fn with_filter(mut self, filter: DataType) -> Self {
        self.filter = Some(filter);
        self
    }

    fn as_computed(&self) -> Computed {
        todo!()
    }

    fn from_codec(&self, codec: &dyn ConstrainableCodec) -> DataType {
        let derived = codec.encodeable_data_types();
        self.filter.map_or(derived, |type_filter| derived & type_filter)
    }
}

pub trait Foo {
    /// The instance path to the codec configuration for the sink, if required for the input constraint.
    fn codec_path() -> Option<InstancePath> {
        None
    }
}

pub trait ConstrainedSinkInput {
    /// The configured codec for the sink, if required for the input constraint.
    fn codec(&self) -> Option<&dyn ConstrainableCodec> {
        None
    }

    /// The instance path to the codec configuration for the sink, if required for the input constraint.
    fn codec_path() -> Option<InstancePath>
    where
        Self: Sized,
    {
        None
    }
}

/// A constrained sink.
pub trait ConstrainedSink: ConstrainedSinkInput {
    /// Gets the input constraint for this sink.
    fn inputs() -> SinkInputConstraint;
}

/// Input constraint for a sink.
pub enum SinkInputConstraint {
    /// The input data type is static and known at compile-time.
    Fixed(DataType),

    /// The input data type is derived from the codec configuration of the sink.
    Derived(DerivedCodecConstraint),
}

impl SinkInputConstraint {
    fn as_computed(&self) -> Computed {
        todo!()
    }

    fn from_sink<S>(&self, sink: S) -> DataType
    where
        S: ConstrainedSinkInput + Sized,
    {
        match self {
            Self::Fixed(data_type) => *data_type,
            Self::Derived(derived) => {
                let codec = sink.codec().expect("sink must expose codec when input constraint is derived");
                derived.from_codec(codec)
            },
        }
    }
}
