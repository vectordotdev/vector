use tokio_util::codec::LengthDelimitedCodec;
use vector_config::configurable_component;

/// Options for building a `LengthDelimitedDecoder` or `LengthDelimitedEncoder`.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LengthDelimitedCoderOptions {
    /// Maximum frame length
    #[serde(default = "default_max_frame_length")]
    pub max_frame_length: usize,

    /// Number of bytes representing the field length
    #[serde(default = "default_length_field_length")]
    pub length_field_length: usize,

    /// Number of bytes in the header before the length field
    #[serde(default = "default_length_field_offset")]
    pub length_field_offset: usize,

    /// Length field byte order (little or big endian)
    #[serde(default = "default_length_field_is_big_endian")]
    pub length_field_is_big_endian: bool,
}

const fn default_max_frame_length() -> usize {
    8 * 1_024 * 1_024
}

const fn default_length_field_length() -> usize {
    4
}

const fn default_length_field_offset() -> usize {
    0
}

const fn default_length_field_is_big_endian() -> bool {
    true
}

impl Default for LengthDelimitedCoderOptions {
    fn default() -> Self {
        Self {
            max_frame_length: default_max_frame_length(),
            length_field_length: default_length_field_length(),
            length_field_offset: default_length_field_offset(),
            length_field_is_big_endian: default_length_field_is_big_endian(),
        }
    }
}

impl LengthDelimitedCoderOptions {
    pub fn build_codec(&self) -> LengthDelimitedCodec {
        let mut builder = tokio_util::codec::LengthDelimitedCodec::builder()
            .length_field_length(self.length_field_length)
            .length_field_offset(self.length_field_offset)
            .max_frame_length(self.max_frame_length)
            .to_owned();
        if self.length_field_is_big_endian {
            builder.big_endian();
        } else {
            builder.little_endian();
        };
        builder.new_codec()
    }
}
