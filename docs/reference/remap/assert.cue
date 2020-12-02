package metadata

remap: functions: assert: {
	arguments: [
		{
			name:        "condition"
			description: "The condition to check."
			required:    true
			type: ["boolean"]
		},
		{
			name:        "message"
			description: "Should condition be false, message will be reported as the failure message."
			required:    true
			type: ["string"]
		},
	]
	return: ["null"]
	category: "event"
	description: #"""
			Checks a given condition. If that condition evaluates to false the event is aborted with
			an error message provided.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				foo: #"bar"#
			}
			source: #"""
				assert(.foo == "buzz", message = "Foo must be buzz!")
				"""#
			output: {
				error: "Foo must be buzz!"
			}
		},
	]
}
