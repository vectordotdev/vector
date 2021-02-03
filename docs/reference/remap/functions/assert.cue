package metadata

remap: functions: assert: {
	category: "Debug"
	description: """
		Asserts the `condition`.

		If the `condition` evaluates to `false` the program is aborted with the `message`.
		"""
	notices: [
		"""
			This function is designed to be used in a standalone fashion, aborting the script if it fails. It should
			not be used in logical expressions.
			""",
	]

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
	internal_failure_reasons: [
		"`condition` evaluates to `false`",
	]
	return: types: ["null"]

	examples: [
		{
			title: "Assertion (true)"
			source: #"""
				assert("foo" == "foo", message: "Foo must be foo!")
				"""#
			return: null
		},
		{
			title: "Assertion (false)"
			source: #"""
				assert("foo" == "bar", message: "Foo must be foo!")
				"""#
			raises: runtime: "Foo must be foo!"
		},
	]
}
