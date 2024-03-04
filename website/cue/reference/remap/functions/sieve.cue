package metadata

remap: functions: sieve: {
	category: "String"
	description: """
		Keeps only mathces of `pattern` in `value`.

		This can be used to list out characters (or patterns) that are allowed in the string and
		remove everything else.
		"""

	arguments: [
		{
			name:        "value"
			description: "The original string."
			required:    true
			type: ["string"]
		},
		{
			name:        "pattern"
			description: """
				Keep all matches of this pattern. Can be a static string or a regular expression. Static string is used as a list of allowed characters.
				"""
			required:    true
			type: ["regex", "string"]
		},
		{
			name:        "replace_sinle"
			description: """
				The string to use to replace single rejected characters.
				"""
			required:    false
			default:	 ""
			type: ["string"]
		},
		{
			name:        "replace_repeated"
			description: """
				The string to use to replace multiple sequential instances of rejected characters.
				"""
			required:    false
			default:	 ""
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Sieve with regex"
			source: #"""
				sieve("test123%456.فوائد.net.", r'[a-z]')
				"""#
			return: "test123456..net."
		},
		{
			title: "Sieve with string"
			source: #"""
				sieve("vector.dev", "eov")
				"""#
			return: "veoev"
		},
		{
			title: "Custom replacements"
			source: #"""
				sieve("test123%456.فوائد.net.", r'[a-z.0-9]', replace_single: "X", replace_repeated: "<REMOVED>")
				"""#
			return: "test123X456.<REMOVED>.net."
		},
	]
}
