package metadata

remap: functions: set_metadata_field: {
	category: "Event"
	description: """
		Sets the given field in the event metadata to the provided value.
		"""

	arguments: [
		{
			name:        "key"
			description: "The name of the field to set in the metadata."
			required:    true
			enum: {
				datadog_api_key: """
					The Datadog API key.

					This field will be used by the  `datadog_*` sinks as the API key to send the events with.
					"""
				splunk_hec_token: """
					The Splunk HEC token.

					This field will be used by the  `splunk_*` sinks as the token to send the events with.
					"""
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
			title: "Set the Datadog API key in the event metadata to the given value."
			source: #"""
				set_metadata_field!("datadog_api_key", "abc122")
				"""#
			return: "null"
		},
	]
}
