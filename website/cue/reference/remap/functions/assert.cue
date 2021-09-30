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

	arguments: [
		{
			name:        "condition"
			description: "The condition to check."
			required:    true
			type: ["boolean"]
		},
		{
			name:        "message"
			description: """
				The failure message that's reported if `condition` evaluates to `false`. If
				unspecified, `"assertion failed"` is used as a default failure message. For example,
				the expression `assert!(1 == 2)` (with no `message` specified) would yield this
				output:

				```text
				function call error for "assert" at (0:15): assertion failed
				```
				"""
			required:    false
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
				ok, err = assert("foo" == "foo", message: "\"foo\" must be \"foo\"!")
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
