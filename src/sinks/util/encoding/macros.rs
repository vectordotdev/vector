/// Generates a customized encoding configuration enum for sinks.
///
/// ## Purpose
///
/// While there are existing encoding enums, such as `StandardEncodings`, some sinks only support a certain subset
/// of those actual encodings, in terms of what the downstream service/API will accept. This macro allows a caller to
/// craft a customized encoding enum that only supports the actual encodings they specify.
///
/// Here's a simple example of how to use the macro and what it will generate as a result:
///
/// ```norun
/// // This macro invocation generates an enum called `MyEncoding` with two variants, `Text` and `Json`,
/// // which map to newline-delimited plaintext and non-delimited JSON, repsectively.
/// generate_custom_encoding_configuration!(MyEncoding {
///     Text,
///     Json,
/// });
/// ```
/// Additionally, and perhaps more important, we automatically generate implementations of `EncodingConfigMigrator`, and
/// `EncodingConfigWithFramingMigrator`, which can then be used in the configuration of the sink like so:
///
/// ```norun
/// struct SinkConfig {
///     // The name of the migrator type is always <name of enum> + `Migrator`.
///     framed_encoding: EncodingConfigWithFramingAdapter<EncodingConfig<MyEncoding>, MyEncodingMigrator>,
///
///     // When using the unframed adapter, the framing information is simply dropped.
///     unframed_encoding: EncodingConfigAdapter<EncodingConfig<MyEncoding>, MyEncodingMigrator>,
/// }
/// ```
///
/// This macro supports two sets of codecs: known and custom. Known and custom codecs can be mixed together in a single
/// macro call.
///
/// ## Known codecs
///
/// Known codecs are codecs that have a fixed framer/serializer, such as text, or newline-delimited JSON,
/// and so on. Simply by using the common identifier for them, the macro handles specifying the correct framer/serializer.
///
/// Known codecs are:
///
/// - `Text` (plaintext, newline delimited)
/// - `Json` (JSON, no delimiter)
/// - `Ndjson` (JSON, newline delimited)
/// - `Native` (Protocol Buffers, no delimiter)
/// - `NativeJson` (JSON, no delimiter)
/// - `Logfmt` (logfmt, no delimiter)
///
/// ## Custom codecs
///
/// In some cases, a sink may need to slightly customize an aspect of a known codec, or specify a totally custom
/// framer/serializer unique to their sink. The macro supports specifying the framer and serializer to use for the codec
/// in a familiar form:
///
/// ```norun
/// // This is a special encoding that encodes to plaintext but doesn't want any delimiters at all.
/// generate_custom_encoding_configuration!(MyEncoding {
///     RawText => (None, codecs::TextSerializerConfig::new().into())
/// })
/// ```
#[macro_export]
macro_rules! generate_custom_encoding_configuration {
    // These arms are provide the framer/serializer for known codecs such as text, JSON, and NDJSON.
    //
    // The reason we have them as arms in this macro, versus another more aptly-named macro/function, is that it lets us
    // avoid scoping issues where callers would have to import other items into scope for the macro to work since you
    // would need a priori knowledge of what's missing, as macros can't curry that information to language servers/IDE
    // helpers, etc.
    (@frame_ser text) => {
        (
            Some(::codecs::NewlineDelimitedEncoderConfig::new().into()),
            ::codecs::TextSerializerConfig::new().into(),
        )
    };

    (@frame_ser json) => {
        (
            None,
            ::codecs::JsonSerializerConfig::new().into(),
        )
    };

    (@frame_ser ndjson) => {
        (
            Some(::codecs::NewlineDelimitedEncoderConfig::new().into()),
            ::codecs::JsonSerializerConfig::new().into(),
        )
    };

    (@frame_ser native) => {
        (
            None,
            ::codecs::NativeSerializerConfig::new().into(),
        )
    };

    (@frame_ser nativejson) => {
        (
            None,
            ::codecs::NativeJsonSerializerConfig::new().into(),
        )
    };

    (@frame_ser logfmt) => {
        (
            None,
            ::codecs::LogfmtSerializerConfig::new().into(),
        )
    };

    // Final step.
    //
    // All codecs, known or custom, have been handled and now we're simply dealing with generating the enum itself, and
    // implementing the migrator traits. We explicitly use well-qualified type paths because macros can't import items
    // on the caller's behalf, even if they're imported in the module where the macro itself is declared.
    (@done; $enum_name:ident { $($codec:ident => $frame_ser:expr,)+ }) => {
        ::paste::paste! {
            #[::vector_config::configurable_component]
            #[derive(Clone, Copy, Debug, Eq, PartialEq)]
            #[serde(rename_all = "snake_case")]
            #[doc = "Supported encodings for the sink."]
            pub enum $enum_name {
                $(
                    #[doc = $codec " encoding."]
                    $codec,
                )+
            }

            impl $enum_name {
                /// Gets this encoding as an encoding configuration adapter.
                #[allow(dead_code)]
                pub fn as_config_adapter(
                    self,
                ) -> $crate::sinks::util::encoding::EncodingConfigAdapter<$crate::sinks::util::encoding::EncodingConfig<Self>, [<$enum_name Migrator>]>
                {
                    let legacy_config: $crate::sinks::util::encoding::EncodingConfig<Self> = self.into();
                    legacy_config.into()
                }

                /// Gets this encoding as a framed encoding configuration adapter.
                #[allow(dead_code)]
                pub fn as_framed_config_adapter(
                    self,
                ) -> $crate::sinks::util::encoding::EncodingConfigWithFramingAdapter<$crate::sinks::util::encoding::EncodingConfig<Self>, [<$enum_name Migrator>]>
                {
                    let legacy_config: $crate::sinks::util::encoding::EncodingConfig<Self> = self.into();
                    legacy_config.into()
                }
            }

            #[derive(Clone, Copy, Debug, ::serde::Serialize, ::serde::Deserialize)]
            #[doc = "Encoding migrator for the `" $enum_name "`."]
            pub struct [<$enum_name Migrator>];

            impl $crate::sinks::util::encoding::EncodingConfigWithFramingMigrator for [<$enum_name Migrator>] {
                type Codec = $enum_name;

                fn migrate(codec: &Self::Codec) -> (Option<::codecs::encoding::FramingConfig>, ::codecs::encoding::SerializerConfig) {
                    match codec {
                        $(
                            $enum_name::$codec => $frame_ser,
                        )+
                    }
                }
            }

            impl $crate::sinks::util::encoding::EncodingConfigMigrator for [<$enum_name Migrator>] {
                type Codec = $enum_name;

                fn migrate(codec: &Self::Codec) -> ::codecs::encoding::SerializerConfig {
                    let (_maybe_framer, serializer) = <Self as $crate::sinks::util::encoding::EncodingConfigWithFramingMigrator>::migrate(codec);

                    serializer
                }
            }
        }
    };

    ($enum_name:ident { $($codec:ident => $frame_ser:expr,)+ } {}) => {
        generate_custom_encoding_configuration!(@done; $enum_name {
            $($codec => $frame_ser,)+
        });
    };

    // Handles custom codecs where the framing and serializer are given directly by the caller.
    //
    // This variant exists because you can't optionally match a trailing comma after an expression when followed by a TT muncher.
    ($enum_name:ident { $($codec:ident => $frame_ser:expr,)* } { $next_codec:ident => $custom_frame_ser:expr, $($tail:tt)* }) => {
        generate_custom_encoding_configuration!($enum_name {
            $($codec => $frame_ser,)*
            $next_codec => $custom_frame_ser,
        } { $($tail)* });
    };

    // Handles custom codecs where the framing and serializer are given directly by the caller.
    //
    // This variant is the inverse of its sibling, for when a custom codec has no trailing comma.
    ($enum_name:ident { $($codec:ident => $frame_ser:expr,)* } { $next_codec:ident => $custom_frame_ser:expr }) => {
        generate_custom_encoding_configuration!($enum_name {
            $($codec => $frame_ser,)*
            $next_codec => $custom_frame_ser,
        } {});
    };

    // Handles "known" codecs which have a predefined framing/serializer combination.
    //
    // This variant exists because you can't optionally match a trailing comma after the next codec ident, otherwise we
    // end up with multiple possible parsing options, which leads to a "local ambiguity when calling macro" error.
    ($enum_name:ident { $($codec:ident => $frame_ser:expr,)* } { $next_codec:ident, $($tail:tt)* }) => {
        ::paste::paste! {
            generate_custom_encoding_configuration!($enum_name {
                $($codec => $frame_ser,)*
                $next_codec => generate_custom_encoding_configuration!(@frame_ser [<$next_codec:lower>]),
            } { $($tail)* });
        }
    };

    // Handles "known" codecs which have a predefined framing/serializer combination.
    //
    // This variant is the inverse of its sibling, for when a known codec has no trailing comma.
    ($enum_name:ident { $($codec:ident => $frame_ser:expr,)* } { $next_codec:ident }) => {
        ::paste::paste! {
            generate_custom_encoding_configuration!($enum_name {
                $($codec => $frame_ser,)*
                $next_codec => generate_custom_encoding_configuration!(@frame_ser [<$next_codec:lower>]),
            } {});
        }
    };

    // Entrypoint.
    //
    // This is normal invocation, which is designed to look as if the input to the macro is the desired enum.
    ($enum_name:ident { $($tail:tt)+ }) => {
        generate_custom_encoding_configuration!($enum_name {} { $($tail)+ });
    };
}
