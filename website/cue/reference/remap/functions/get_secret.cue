package metadata

remap: functions: get_secret: {
	category: "Event"
	description: """
		Returns the value of the given secret from an event.
		"""

	arguments: [
		{
			name: "key"
			description: """
				The name of the secret.
				"""
			required: true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
	]
	return: types: ["string"]

	examples: [
		{
			title: "Get the Datadog API key from the event metadata"
			source: #"""
				get_secret("datadog_api_key")
				"""#
			return: "secret value"
		},
	]
}
