package metadata

remap: functions: set_semantic_meaning: {
	category: "Event"
	description: """
		Sets a semantic meaning for an event. **Note**: This function assigns
		meaning at startup, and has _no_ runtime behavior. It is suggested
		to put all calls to this function at the beginning of a VRL function. The function
		cannot be conditionally called. For example, using an if statement cannot stop the meaning
		from being assigned.
		"""

	arguments: [
		{
			name: "target"
			description: """
				The path of the value that is assigned a meaning.
				"""
			required: true
			type: ["path"]
		},
		{
			name: "meaning"
			description: """
				The name of the meaning to assign.
				"""
			required: true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
	]
	return: types: ["null"]

	examples: [
		{
			title: "Sets custom field semantic meaning"
			source: #"""
				set_semantic_meaning(.foo, "bar")
				"""#
			return: null
		},
	]
}
