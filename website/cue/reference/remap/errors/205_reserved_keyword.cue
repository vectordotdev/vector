package metadata

remap: errors: "205": {
	title: "Reserved keyword"
	description: """
		You've used a name for a variable that serves another purpose in VRL or is reserved for potential future use.
		"""
	resolution: """
		Use a different variable name.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				else = "some value"
				"""#
			diff: #"""
				-else = "some value"
				+some_non_reserved_name = "some value"
				"""#
		},
	]
}
