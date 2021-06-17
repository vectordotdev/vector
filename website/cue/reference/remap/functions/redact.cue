package metadata

remap: functions: redact: {
	category:    "String"
	description: """
		Redact sensitive data in `value` such as:

		- [US social security card numbers](\(urls.us_social_security_number))
		- and other forms of personally identifiable information via custom patterns
		- (more to come!)

		This can help achieve compliance by ensuring sensitive data never leaves your network.
		"""

	arguments: [
		{
			name: "value"
			description: #"""
				The value to redact sensitive data from.

				Its behavior differs depending on the type of `value`:

				- For strings, it simply redacts the sensitive data and returns a new string
				- For arrays, it redacts the sensitive data in each string element
				- For objects, it masks the sensitive data in each string value, but not keys

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

				Each filter can be specified in one of three ways:

				- As a regular expression directly, which will be used to redact text matching it
				- As an object with a `type` key that corresponds to a named filter and additional keys for customizing that filter
				- As a named filter, if it has no required parameters

				Named filters are:

				- `pattern`: Redact text matching any regular expressions specified in the, required, `patterns`
					key. This is the expanded form of just passing a regular expression as a filter.
				- `us_social_security_number`: Redact US social security card numbers.

				See examples for more details.

				This parameter must be a static expression. You cannot use variables or other dynamic expressions
				with it. This allows us to validate the argument at compile-time to avoid runtime errors.
				"""#
			required: false
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
