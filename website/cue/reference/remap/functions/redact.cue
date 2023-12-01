package metadata

remap: functions: redact: {
	category:    "String"
	description: """
		Redact sensitive data in `value` such as:

		- [US social security card numbers](\(urls.us_social_security_number))
		- Other forms of personally identifiable information with custom patterns

		This can help achieve compliance by ensuring sensitive data does not leave your network.
		"""

	arguments: [
		{
			name: "value"
			description: #"""
				The value to redact sensitive data from.

				The function's behavior depends on the type of `value`:

				- For strings, the sensitive data is redacted and a new string is returned.
				- For arrays, the sensitive data is redacted in each string element.
				- For objects, the sensitive data in each string value is masked, but the keys are not masked.

				For arrays and objects, the function recurses into any nested arrays or objects. Any non-string elements are
				skipped.

				Redacted text is replaced with `[REDACTED]`.
				"""#
			required: true
			type: ["string", "object", "array"]
		},
		{
			name: "filters"
			description: #"""
				List of filters applied to `value`.

				Each filter can be specified in the following ways:

				- As a regular expression, which is used to redact text that match it.
				- As an object with a `type` key that corresponds to a named filter and additional keys for customizing that filter.
				- As a named filter, if it has no required parameters.

				Named filters can be a:

				- `pattern`: Redacts text matching any regular expressions specified in the `patterns`
					key, which is required. This is the expanded version of just passing a regular expression as a filter.
				- `us_social_security_number`: Redacts US social security card numbers.

				See examples for more details.

				This parameter must be a static expression. This to allow validation of the argument at compile-time
				to avoid runtime errors. You cannot use variables or other dynamic expressions
				with it.
				"""#
			required: true
			type: ["array"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string", "object", "array"]

	examples: [
		{
			title: "Replace text using a regex"
			source: #"""
				redact("my id is 123456", filters: [r'\d+'])
				"""#
			return: "my id is [REDACTED]"
		},
		{
			title: "Replace us social security numbers in any field"
			source: #"""
				redact({ "name": "John Doe", "ssn": "123-12-1234"}, filters: ["us_social_security_number"])
				"""#
			return: {
				name: "John Doe"
				ssn:  "[REDACTED]"
			}
		},
	]
}
