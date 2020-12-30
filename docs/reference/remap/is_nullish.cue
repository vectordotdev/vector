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
		Determines whether the provided value should be considered "nullish," where nullish
		includes all of the following:

		* An empty string (`""`)
		* A string that only contains whitespace
		* Dash (`"-"`)
		* Newline (`"\n"`)
		* `null`
		"""#
	examples: [
		{
			title: "Blank item"
			input: {
				string_field: "-"
			}
			source: ".is_empty = is_nullish(.string_field)"
			output: {
				string_field: "-"
				is_empty:     true
			}
		},
	]
}
