package metadata

remap: functions: get_event_metadata: {
	category: "System"
	description: """
		Returns the value of the given field from the event metadata.
		"""

	arguments: [
		{
			name:        "key"
			description: "The name of the key to look up in the metadata."
			required:    true
			enum: {
				"datadog_api_key": "The Datadog api key."
			}
			type: ["string"]
		},
	]
	internal_failure_reasons: [
	]
	return: types: ["string"]

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
