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
	return: ["string"]
	category: "String"
	description: """
		Obscures sensitive data, such as personal identification numbers or credit card numbers, in
		Vector event data.
		"""
	examples: [
		{
			title: "Redact credit card number"
			input: log: credit_card: "9876123454320123"
			source: """
				.credit_card = redact(.credit_card, filters: ["pattern"], redactor: "full", patterns: [/[0-9]{16}/])
				"""
			output: log: credit_card: "****"
		},
		{
			title: "Redact email address"
			input: log: email: "ana@booper.com"
			source: #"""
				.email = redact(.email, filters: ["pattern"], redactor: "full", patterns: [/^\S+@\S+$/])
				"""#
			output: log: email: "****"
		},
	]
}
