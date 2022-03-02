package metadata

remap: functions: remove_metadata_field: {
	category: "Event"
	description: """
		Removes the value of the given field from the event metadata.
		"""

	arguments: [
		{
			name: "key"
			description: """
				The name of the field to look up in the metadata.
				"""
			required: true
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
	]
	internal_failure_reasons: [
	]
	return: types: ["null"]

	examples: [
		{
			title: "Removes the Datadog API key from the event metadata."
			source: #"""
				remove_metadata_field!("datadog_api_key")
				"""#
			return: "null"
		},
	]
}
