package metadata

remap: functions: redact: {
	arguments: [
		{
			name:        "value"
			description: "The value that you want to redact."
			required:    true
			type: ["string"]
		},
		{
			name:        "filters"
			description: "A list of filters to apply to the input value."
			required:    false
			type: ["array"]
			enum: {
				pattern: "Filter based on a supplied regular expression."
			}
		},
		{
			name:        "redactor"
			description: "The redaction method to be applied, with multiple options available."
			required:    false
			type: ["string"]
			enum: {
				full: "Replace the entire content with `****`. Exactly 4 characters are used as to not give away the length of the original value."
			}
		},
		{
			name: "patterns"
			description: """
				A list of patterns to apply. Patterns can be strings or regular expressions; if a
				string is supplied, Vector searches for exact matches to redact.
				"""
			required: false
			type: ["array"]
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "String"
	description: """
		Redacts sensitive data in the provided `value` via the specified `patterns`.

		This function is useful to redact personally identifiable information (PII) such as emails, credit card numbers,
		and more.
		"""
	examples: [
		{
			title: "Redact (credit card number)"
			source: """
				redact("9876123454320123", filters: ["pattern"], redactor: "full", patterns: [/[0-9]{16}/])
				"""
			return: "****"
		},
		{
			title: "Redact (email address)"
			source: #"""
				redact("vic@vector.dev", filters: ["pattern"], redactor: "full", patterns: [/^\S+@\S+$/])
				"""#
			return: "****"
		},
	]
}
