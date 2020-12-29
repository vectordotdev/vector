package metadata

remap: functions: is_blank: {
	arguments: [
		{
			name:        "value"
			description: "The value to check for blankness"
			required:    true
			type: ["string", "null"]
		},
	]
	return: ["boolean"]
	category: "Check"
	description: #"""
		Determines whether the provided value should be considered blank, where blank is defined as
		one of the following:

		* An empty string (`""`)
		* A string that only contains whitespace (regardless of the string's length)
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
			source: ".is_empty = is_blank(.string_field)"
			output: {
				string_field: "-"
				is_empty:     true
			}
		},
	]
}
