package metadata

base: components: transforms: filter: configuration: condition: {
	description: """
		The condition that every input event is matched against.

		If an event is matched by the condition, it is forwarded. Otherwise, the event is dropped.
		"""
	required: true
	type: condition: {}
}
