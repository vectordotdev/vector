package metadata

remap: functions: is_nullish: {
	arguments: [
		{
			name:        "value"
			description: #"The value to check for "nullishness," i.e. a useless value."#
			required:    true
			type: ["any"]
		},
	]
	return: ["boolean"]
	category: "Check"
	description: #"""
		Determines whether the provided value should be considered "nullish," that is, to indicate
		the absence of a meaningful value. The following are considered nullish in VRL:

		* `null`
		* A single dash (`"-"`)
		* Any string that contains only whitespace characters as defined by the the [Unicode
		  definition of the `White_Space` property](\(urls.unicode_whitespace)). That includes
		  empty strings (`""`), common characters like `"\n"`, "\r", and others.

		All values of any other type return `false`.
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
