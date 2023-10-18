package metadata

remap: functions: set_secret: {
	category: "Event"
	description: """
		Sets the given secret in the event.
		"""

	arguments: [
		{
			name:        "key"
			description: "The name of the secret."
			required:    true
			type: ["string"]
		},
		{
			name:        "secret"
			description: "The secret value."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
	]
	return: types: ["null"]

	examples: [
		{
			title: "Set the Datadog API key to the given value"
			source: #"""
				set_secret("datadog_api_key", "abc122")
				"""#
			return: null
		},
	]
}
