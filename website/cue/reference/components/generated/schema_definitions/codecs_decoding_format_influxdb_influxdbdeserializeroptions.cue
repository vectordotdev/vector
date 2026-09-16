package metadata

_schemaDefinitions: "codecs::decoding::format::influxdb::InfluxdbDeserializerOptions": object: options: lossy: {
	description: """
		Determines whether to replace invalid UTF-8 sequences instead of failing.

		When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

		[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
		"""
	required: false
	type: bool: default: true
}
