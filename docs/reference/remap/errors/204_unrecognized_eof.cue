package metadata

remap: errors: "204": {
	title:       "Unrecognized end-of-file (EOF)"
	description: """
		Your VRL program contains an [EOF](\(urls.eof)) character that unexpectedly ends the program.
		"""
	rationale:   null
	resolution: """
		Remove the EOF character.
		"""
}
