package metadata

remap: functions: is_nullish: {
	arguments: [
		{
			name:        "value"
			description: #"The value to check for "nullishness", i.e. a useless value."#
			required:    true
			type: ["string", "null"]
		},
	]
	return: ["boolean"]
	category: "Check"
	description: #"""
		Determines whether the provided value should be considered "nullish," that is, to indicate
		the absence of a meaningful value. The following are considered nullish in VRL:

		* An empty string (`""`)
		* A string that only contains whitespace
		* A single dash (`"-"`)
		* Newline (`"\n"`)
		* Carriage return (`"\r"`)
		* `null`

		If your use case requires a different conception of nullish, we recommend using more
		specific checks. If only empty string is considered nullish in your domain, for example,
		then a check like `.field == ""` would suffice.
		"""#
	examples: [
		{
			title: "Empty string"
			input: {
				string_field: ""
			}
			source: ".is_empty = is_nullish(.string_field)"
			output: {
				string_field: ""
				is_empty:     true
			}
		},
	]
}
