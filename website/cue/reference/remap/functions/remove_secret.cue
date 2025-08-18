package metadata

remap: functions: remove_secret: {
	category: "Event"
	description: """
		Removes a secret from an event.
		"""

	arguments: [
		{
			name: "key"
			description: """
				The name of the secret to remove.
				"""
			required: true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["null"]

	examples: [
		{
			title: "Removes the Datadog API key from the event"
			source: #"""
				remove_secret("datadog_api_key")
				"""#
			return: null
		},
	]
}
