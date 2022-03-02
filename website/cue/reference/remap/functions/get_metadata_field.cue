package metadata

remap: functions: get_metadata_field: {
	category: "Event"
	description: """
		Returns the value of the given field from the event metadata.
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

					This exists if the `store_api_key` setting is true in the `datadog_agent` source.
					"""
				splunk_hec_token: """
					The Splunk HEC token.

					This exists if the `store_hec_token` setting is true in the `splunk_hec` source.
					"""
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
				get_metadata_field!("datadog_api_key")
				"""#
			return: "abc123"
		},
	]
}
