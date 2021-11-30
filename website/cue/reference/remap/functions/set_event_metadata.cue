package metadata

remap: functions: set_event_metadata: {
	category: "System"
	description: """
		Sets the given field in the event metadata to the provided value.
		"""

	arguments: [
		{
			name:        "key"
			description: "The name of the key to set in the metadata."
			required:    true
			enum: {
				"datadog_api_key": "The Datadog api key."
			}
			type: ["string"]
		},
                {
			name:        "value"
			description: "The value to set the field to."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
	]
	return: types: ["null"]

	examples: [
		{
			title: "Get the Datadog API key from the event metadata."
			source: #"""
				get_event_metadata!("datadog_api_key")
				"""#
			return: "abc123"
		},
	]
}
