package metadata

remap: functions: set_semantic_meaning: {
	category: "Event"
	description: """
		Sets a semantic meaning for an event. Note that this function assigns
		meaning at Vector startup, and has _no_ runtime behavior. It is suggested
		to put all calls to this function at the beginning of a VRL function. The function
		cannot be conditionally called (eg: using an if statement cannot stop the meaning
		from being assigned).
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
			title: "Get the Datadog API key from the event metadata."
			source: #"""
				get_secret("datadog_api_key")
				"""#
			return: "secret value"
		},
	]
}
