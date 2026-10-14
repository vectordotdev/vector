package metadata

_schemaDefinitions: "vector_core::serde::BoolOrStruct<vector_core::config::SourceAcknowledgementsConfig>": {
	bool: {}
	object: options: enabled: {
		description: "Whether or not end-to-end acknowledgements are enabled for this source."
		required:    false
		type: bool: {}
	}
}
