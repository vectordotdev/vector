//! A collection of support structures that are used in the process of encoding
//! events into bytes.

pub mod chunking;
pub mod format;
pub mod framing;
pub mod serializer;
pub use chunking::{Chunker, Chunking, GelfChunker};
pub use format::{
    AvroSerializer, AvroSerializerConfig, AvroSerializerOptions, CefSerializer,
    CefSerializerConfig, CsvSerializer, CsvSerializerConfig, GelfSerializer, GelfSerializerConfig,
    JsonSerializer, JsonSerializerConfig, JsonSerializerOptions, LogfmtSerializer,
    LogfmtSerializerConfig, NativeJsonSerializer, NativeJsonSerializerConfig, NativeSerializer,
    NativeSerializerConfig, ProtobufSerializer, ProtobufSerializerConfig,
    ProtobufSerializerOptions, RawMessageSerializer, RawMessageSerializerConfig, SyslogSerializer,
    SyslogSerializerConfig, TextSerializer, TextSerializerConfig,
};
#[cfg(feature = "opentelemetry")]
pub use format::{OtlpSerializer, OtlpSerializerConfig};
pub use framing::{
    BoxedFramer, BoxedFramingError, BytesEncoder, BytesEncoderConfig, CharacterDelimitedEncoder,
    CharacterDelimitedEncoderConfig, CharacterDelimitedEncoderOptions, Framer, FramingConfig,
    LengthDelimitedEncoder, LengthDelimitedEncoderConfig, NewlineDelimitedEncoder,
    NewlineDelimitedEncoderConfig, VarintLengthDelimitedEncoder,
    VarintLengthDelimitedEncoderConfig,
};
pub use serializer::{Serializer, SerializerConfig};

/// An error that occurred while building an encoder.
pub type BuildError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// An error that occurred while encoding structured events into byte frames.
#[derive(Debug)]
pub enum Error {
    /// The error occurred while encoding the byte frame boundaries.
    FramingError(BoxedFramingError),
    /// The error occurred while serializing a structured event into bytes.
    SerializingError(vector_common::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FramingError(error) => write!(formatter, "FramingError({error})"),
            Self::SerializingError(error) => write!(formatter, "SerializingError({error})"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::FramingError(Box::new(error))
    }
}
