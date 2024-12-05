package metadata

remap: syntax: expressions: {
	title: "Expressions"
	description: """
		VRL programs are made up of literal and dynamic expressions, described more in detail below.  Expressions can be separated by newline, or by using a semicolon.
		"""

	examples: [
		"""
		# newline delimited expressions 
		del(.user_info)
		.timestamp = now()  
		""",
		"""
		# semicolon delimited expressions
		del(.user_info); .timestamp = now()
		"""
	]
}
