package metadata

remap: errors: "640": {
	title:       "Fallible argument"
	description: """
		All assignments in VRL need to be to either a path or a variable. Assigning to
		"""

	resolution: """
		Assign the right-hand-side value to either a variable or a path.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				_ = "the hills are alive"
				"""#
			raises: compiletime: #"""
				error: \#(title)
				┌─ :1:5
				│
				1 │ _ = "the hills are alive"
				│ --- ^^^^^^^^^^^^^^^^^^^^^ this no-op assignment is useless
				│ │
				│ or remove the assignment
				│ either assign to a path or variable here
				│
				"""#
			diff: #"""
				- 	_ = "the hills are alive"
				+# 	.movie_song_quote = "the hills are alive"
				"""#
		},
	]
}
