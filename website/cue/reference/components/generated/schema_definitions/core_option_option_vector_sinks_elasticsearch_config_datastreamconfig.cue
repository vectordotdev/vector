package metadata

_schemaDefinitions: "core::option::Option<vector::sinks::elasticsearch::config::DataStreamConfig>": object: options: {
	auto_routing: {
		description: """
			Automatically routes events by deriving the data stream name using specific event fields.

			The format of the data stream name is `<type>-<dataset>-<namespace>`, where each value comes
			from the `data_stream` configuration field of the same name.

			If enabled, the value of the `data_stream.type`, `data_stream.dataset`, and
			`data_stream.namespace` event fields are used if they are present. Otherwise, the values
			set in this configuration are used.
			"""
		required: false
		type: bool: default: true
	}
	dataset: {
		description: "The data stream dataset used to construct the data stream at index time."
		required:    false
		type: string: {
			default: "generic"
			examples: ["generic", "nginx", "{{ service }}"]
			syntax: "template"
		}
	}
	namespace: {
		description: "The data stream namespace used to construct the data stream at index time."
		required:    false
		type: string: {
			default: "default"
			examples: ["{{ environment }}"]
			syntax: "template"
		}
	}
	sync_fields: {
		description: """
			Automatically adds and syncs the `data_stream.*` event fields if they are missing from the event.

			This ensures that fields match the name of the data stream that is receiving events.
			"""
		required: false
		type: bool: default: true
	}
	type: {
		description: "The data stream type used to construct the data stream at index time."
		required:    false
		type: string: {
			default: "logs"
			examples: ["metrics", "synthetics", "{{ type }}"]
			syntax: "template"
		}
	}
}
