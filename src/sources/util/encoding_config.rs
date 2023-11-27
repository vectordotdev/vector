use vector_lib::configurable::configurable_component;

/// Character set encoding.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfig {
    /// Encoding of the source messages.
    ///
    /// Takes one of the encoding [label strings](https://encoding.spec.whatwg.org/#concept-encoding-get) defined as
    /// part of the [Encoding Standard](https://encoding.spec.whatwg.org/).
    ///
    /// When set, the messages are transcoded from the specified encoding to UTF-8, which is the encoding that is
    /// assumed internally for string-like data. Enable this transcoding operation if you need your data to
    /// be in UTF-8 for further processing. At the time of transcoding, any malformed sequences (that can't be mapped to
    /// UTF-8) is replaced with the Unicode [REPLACEMENT
    /// CHARACTER](https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character) and warnings are
    /// logged.
    #[configurable(metadata(docs::examples = "utf-16le"))]
    #[configurable(metadata(docs::examples = "utf-16be"))]
    pub charset: &'static encoding_rs::Encoding,
}
