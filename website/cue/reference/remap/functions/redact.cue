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

				The function's behavior depends on `value`'s type:

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

				This parameter must be a static expression so that the argument can be validated at compile-time
				to avoid runtime errors. You cannot use variables or other dynamic expressions with it.
				"""#
			required: true
			type: ["array"]
		},
		{
			name: "redactor"
			description: #"""
				Specifies what to replace the redacted strings with.

				It is given as an object with a "type" key specifying the type of redactor to use
				and additional keys depending on the type. The following types are supported:

				- `full`: The default. Replace with the string "[REDACTED]".
				- `text`: Replace with a custom string. The `replacement` key is required, and must
				  contain the string that is used as a replacement.
				- `sha2`: Hash the redacted text with SHA-2 as with [`sha2`](\(urls.sha2)). Supports two optional parameters:
					- `variant`: The variant of the algorithm to use. Defaults to SHA-512/256.
					- `encoding`: How to encode the hash as text. Can be base16 or base64.
						Defaults to base64.
				- `sha3`: Hash the redacted text with SHA-3 as with [`sha3`](\(urls.sha3)). Supports two optional parameters:
					- `variant`: The variant of the algorithm to use. Defaults to SHA3-512.
					- `encoding`: How to encode the hash as text. Can be base16 or base64.
						Defaults to base64.


				As a convenience you can use a string as a shorthand for common redactor patterns:

				- `"full"` is equivalent to `{"type": "full"}`
				- `"sha2"` is equivalent to `{"type": "sha2", "variant": "SHA-512/256", "encoding": "base64"}`
				- `"sha3"` is equivalent to `{"type": "sha3", "variant": "SHA3-512", "encoding": "base64"}`

				This parameter must be a static expression so that the argument can be validated at compile-time
				to avoid runtime errors. You cannot use variables or other dynamic expressions with it.
				"""#
			required: false
			type: ["string", "object"]
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
		{
			title: "Replace with custom text"
			source: #"""
				redact("my id is 123456", filters: [r'\d+'], redactor: {"type": "text", "replacement": "***"})
				"""#
			return: "my id is ***"
		},
		{
			title: "Replace with SHA-2 hash"
			source: #"""
				redact("my id is 123456", filters: [r'\d+'], redactor: "sha2")
				"""#
			return: "my id is GEtTedW1p6tC094dDKH+3B8P+xSnZz69AmpjaXRd63I="
		},
		{
			title: "Replace with SHA-3 hash"
			source: #"""
				redact("my id is 123456", filters: [r'\d+'], redactor: "sha3")
				"""#
			return: "my id is ZNCdmTDI7PeeUTFnpYjLdUObdizo+bIupZdl8yqnTKGdLx6X3JIqPUlUWUoFBikX+yTR+OcvLtAqWO11NPlNJw=="
		},
		{
			title: "Replace with SHA-256 hash using hex encoding"
			source: #"""
				redact("my id is 123456", filters: [r'\d+'], redactor: {"type": "sha2", "variant": "SHA-256", "encoding": "base16"})
				"""#
			return: "my id is 8d969eef6ecad3c29a3a629280e686cf0c3f5d5a86aff3ca12020c923adc6c92"
		},
	]
}
