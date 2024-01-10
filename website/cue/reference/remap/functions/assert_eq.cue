package metadata

remap: functions: assert_eq: {
	category: "Debug"

	description: """
		Asserts that two expressions, `left` and `right`, have the same value. The program is
		aborted with `message` if they do not have the same value.
		"""

	notices: [
		"""
			The `assert_eq` function should be used in a standalone fashion and only when you want to
			abort the program. You should avoid it in logical expressions and other situations in which
			you want the program to continue if the condition evaluates to `false`.
			""",
	]

	pure: false

	arguments: [
		{
			name:        "left"
			description: "The value to check for equality against `right`."
			required:    true
			type: ["any"]
		},
		{
			name:        "right"
			description: "The value to check for equality against `left`."
			required:    true
			type: ["any"]
		},
		{
			name: "message"
			description: """
				An optional custom error message. If the equality assertion fails, `message` is
				appended to the default message prefix. See the [examples](#assert_eq-examples)
				below for a fully formed log message sample.
				"""
			required: false
			type: ["string"]
		},
	]

	internal_failure_reasons: []

	return: types: ["boolean"]

	examples: [
		{
			title:  "Successful assertion"
			source: "assert_eq!(1, 1)"
			return: true
		},
		{
			title:  "Unsuccessful assertion"
			source: "assert_eq!(127, [1, 2, 3])"
			raises: runtime: #"function call error for "assert_eq" at (0:26): assertion failed: 127 == [1, 2, 3]"#
		},
		{
			title: "Unsuccessful assertion with custom log message"
			source: #"""
				 assert_eq!(1, 0, message: "Unequal integers")
				"""#
			raises: runtime: #"function call error for "assert_eq" at (1:46): Unequal integers"#
		},
	]
}
