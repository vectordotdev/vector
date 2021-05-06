package metadata

remap: functions: redact: {
	category: "String"
	description: """
		Redact sensitive data in `value`.
		"""

	arguments: [
		{
			name: "value"
			description: #"""
				The value to redact sensitive data from.

				Its behavior differs depending on the type of `value`:
				- For strings it simply redacts the sensitive data and returns a new string
				- For arrays, it redacts the sensitive data in each string element
				- For objects it masks the sensitive data in each value

				For arrays and objects it will recurse into any nested arrays or objects. Any non-string elements will
				be skipped.

				Any redacted text will be replaced with `[REDACTED]`.
				"""#
			required: true
			type: ["string", "object", "array"]
		},
		{
			name: "filters"
			description: #"""
					List of filters to be applied to the `value`.

					If none, the default set of filters is applied: ["credit_card"].

					Each filter can be specified in one of three ways:
					- As a regular expression directly, which will be used to redact text matching it
					- As an object with a `type` key that corresponds to a named filter and additional keys for customizing that filter
					- As a named filter, if it has no required parameters

					Named filters are:
					- `pattern`: Redact text matching any regular expressions specified in the, required, `patterns`
					   key. This is the expanded form of just passing a regular expression as a filter.
					- `credit_card`: Redact credit card numbers.

					See examples for mare.

					Note: This parameter must be a static expression. You cannot use variables or other dynamic
					expressions with it. This allows us to validate the argument at compile-time to avoid runtime
					errors.
				"""#
			required: false
			type: ["array"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string", "object", "array"]

	examples: [
		{
			source: #"""
				redact({ "name": "John Doe", "card_number": "4916155524184782"})
				"""#
			return: {
				name:  "John Doe"
				field: "[REDACTED]"
			}
		},
		{
			title: "Replace text using a regex"
			source: #"""
				redact("my id is 123456", filters: [r'\d+'])
				"""#
			return: "my id is [REDACTED]"
		},
		{
			title: "Replace credit card numbers in any field"
			source: #"""
				redact({ "name": "John Doe", "card_number": "4916155524184782"}, filters: ["credit_card"])
				"""#
			return: {
				name:  "John Doe"
				field: "[REDACTED]"
			}
		},
	]
}
