package metadata

remap: errors: "640": {
	title: "No-op assignment"
	description: """
		You've assigned a value to something that is neither a variable nor a path.
		"""

	rationale: """
		All assignments in VRL need to be to either a path or a variable. If you try to assign a value to, for example,
		underscore (`_`), this operation is considered a "no-op" as it has no effect (and is thus not an assignment at
		all).
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
			diff: #"""
				- 	_ = "the hills are alive"
				+# 	.movie_song_quote = "the hills are alive"
				"""#
		},
	]
}
