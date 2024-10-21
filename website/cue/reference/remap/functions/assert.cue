package metadata

remap: functions: assert: {
	category: "Debug"
	description: """
		Asserts the `condition`, which must be a Boolean expression. The program is aborted with
		`message` if the condition evaluates to `false`.
		"""
	notices: [
		"""
			The `assert` function should be used in a standalone fashion and only when you want to abort the program. You
			should avoid it in logical expressions and other situations in which you want the program to continue if the
			condition evaluates to `false`.
			""",
	]

	pure: false

	arguments: [
		{
			name:        "condition"
			description: "The condition to check."
			required:    true
			type: ["boolean"]
		},
		{
			name: "message"
			description: """
				An optional custom error message. If the equality assertion fails, `message` is
				appended to the default message prefix. See the [examples](#assert-examples) below
				for a fully formed log message sample.
				"""
			required: false
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`condition` evaluates to `false`.",
	]
	return: types: ["null"]

	examples: [
		{
			title: "Assertion (true)"
			source: #"""
				assert!("foo" == "foo", message: "\"foo\" must be \"foo\"!")
				"""#
			return: true
		},
		{
			title: "Assertion (false)"
			source: #"""
				assert!("foo" == "bar", message: "\"foo\" must be \"foo\"!")
				"""#
			raises: runtime: #"function call error for "assert" at (0:60): "foo" must be "foo"!"#
		},
	]
}
