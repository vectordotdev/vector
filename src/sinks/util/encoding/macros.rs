/// Generates a customized encoding configuration enum for sinks.
///
/// While there are existing encoding enums, such as `StandardEncodings`, some sinks only suuport a certain subset
/// of those actual encodings, in terms of what the downstream service/API will accept. This macro allows a caller to
/// craft a customized encoding enum that only supports the actual encodings they specify.
///
/// Here's a simple example of how to use the macro and what it will generate as a result:
///
/// ```norun
/// // This macro invocation generates an enum called `MyEncoding` with two variants, `Text` and `Json`,
/// // which map to newline-delimited plaintext and non-delimited JSON, repsectively.
/// generate_custom_encoding_configuration!(MyEncoding {
/// 	Text,
/// 	Json,
/// });
///
/// // We also have an automatically generated implementation of `EncodingConfigWithFramingMigrator` which can then be
/// // used in the configuration of the sink like so:
/// struct SinkConfig {
///     // The name of the migrator type is always <name of enum> + `WithFramingMigrator`.
///     encoding: EncodingConfigWithFramingAdapter<EncodingConfig<MyEncoding>, MyEncodingWithFramingMigrator>,
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
/// - `JSON` (JSON, no delimiter)
/// - `NDJSON` (JSON, newline delimited)
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
///     RawText => (None, codecs::encoding::format::TextSerializerConfig::new().into())
/// })
/// ```
#[macro_export]
macro_rules! generate_custom_encoding_configuration {
	// TODO: Replace the placeholder doc comment for each enum variant with something actually useful once we figure out
	// why the hell it doesn't seem to let us interpolate/concat/etc anything except for string literals.

	(__frame_ser__known__text) => {
        (
            Some(::codecs::encoding::NewlineDelimitedEncoderConfig::new().into()),
            ::codecs::encoding::JsonSerializerConfig::new().into(),
        )
    };

	(__frame_ser__known__json) => {
        (
            None,
            ::codecs::encoding::TextSerializerConfig::new().into(),
        )
    };

    (__frame_ser__known__ndjson) => {
        (
            Some(::codecs::encoding::NewlineDelimitedEncoderConfig::new().into()),
            ::codecs::encoding::JsonSerializerConfig::new().into(),
        )
    };

	// Final step.
	//
	// All codecs, known or custom, have been handled and now we're simply dealing with generating the enum itself, and
	// implementing the framed migrator trait. We explicitly use well-qualified type paths because macros can't import
	// items on the caller's behalf, even if they're imported in the module where the macro itself is declared.
	(@done $framer_desc:expr, $enum_name:ident { $($codec:ident => $frame_ser:expr,)+ }) => {
		::paste::paste! {
			/// Supported encodings.
			#[::vector_config::configurable_component]
			#[derive(Clone, Debug, Eq, PartialEq)]
			#[serde(rename_all = "snake_case")]
			pub enum $enum_name {
				$(
					/// Placeholder.
					$codec,
				)+
			}

			impl $enum_name {
				/// Gets this encoding as a framed encoding configuration adapter.
				pub fn as_framed_config_adapter(
					&self,
				) -> EncodingConfigWithFramingAdapter<EncodingConfig<Self>, [<$enum_name WithFramingMigrator>]>
				{
					let legacy_config: EncodingConfig<Self> = self.clone().into();
					legacy_config.into()
				}
			}

			#[doc = $framer_desc]
			#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
			pub struct [<$enum_name WithFramingMigrator>];

			impl $crate::sinks::util::encoding::EncodingConfigWithFramingMigrator for [<$enum_name WithFramingMigrator>] {
				type Codec = $enum_name;

				fn migrate(codec: &Self::Codec) -> (Option<::codecs::encoding::FramingConfig>, ::codecs::encoding::SerializerConfig) {
					match codec {
						$(
							$enum_name::$codec => $frame_ser,
						)+
					}
				}
			}
		}
	};

	($enum_name:ident { $($codec:ident => $frame_ser:expr,)+ } ()) => {
		generate_custom_encoding_configuration!(@done concat!("Framing-specific migrator for `", stringify!($enum_name), "`."),
		$enum_name {
			$($codec => $frame_ser,)*
		});
	};

	// Handles custom codecs where the framing and serializer are given directly by the caller.
	//
	// This variant exists because you can't optionally match a trailing comma after an expression when followed by a TT muncher.
	($enum_name:ident { $($codec:ident => $frame_ser:expr,)* } ($next_codec:ident => $custom_frame_ser:expr, $($tail:tt)*)) => {
		generate_custom_encoding_configuration!($enum_name {
			$($codec => $frame_ser,)*
			$next_codec => $custom_frame_ser,
		} ($($tail)*));
	};

	// Handles custom codecs where the framing and serializer are given directly by the caller.
	//
	// This variant is the inverse of its sibling, for when a custom codec has no trailing comma.
	($enum_name:ident { $($codec:ident => $frame_ser:expr,)* } ($next_codec:ident => $custom_frame_ser:expr)) => {
		generate_custom_encoding_configuration!($enum_name {
			$($codec => $frame_ser,)*
			$next_codec => $custom_frame_ser,
		} ());
	};

	// Handles "known" codecs which have a predefined framing/serializer combination.
	//
	// This variant exists because you can't optionally match a trailing comma after the next codec ident, otherwise we
	// end up with multiple possible parsing options, which leads to a "local ambiguity when calling macro" error.
	($enum_name:ident { $($codec:ident => $frame_ser:expr,)* } ($next_codec:ident, $($tail:tt)*)) => {
		::paste::paste! {
			generate_custom_encoding_configuration!($enum_name {
				$($codec => $frame_ser,)*
				$next_codec => generate_custom_encoding_configuration!([<__frame_ser__known__ $next_codec:lower>]),
			} ($($tail)*));
		}
	};

	// Handles "known" codecs which have a predefined framing/serializer combination.
	//
	// This variant is the inverse of its sibling, for when a known codec has no trailing comma.
	($enum_name:ident { $($codec:ident => $frame_ser:expr,)* } ($next_codec:ident)) => {
		::paste::paste! {
			generate_custom_encoding_configuration!($enum_name {
				$($codec => $frame_ser,)*
				$next_codec => generate_custom_encoding_configuration!([<__frame_ser__known__ $next_codec:lower>]),
			} ());
		}
	};

	// Entrypoint.
	//
	// This is normal invocation, which is designed to look as if the input to the macro is the desired enum.
	($enum_name:ident { $($tail:tt)+ }) => {
		generate_custom_encoding_configuration!($enum_name { } ($($tail)+));
	};
}
