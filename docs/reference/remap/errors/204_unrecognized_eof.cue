package metadata

remap: errors: "204": {
	title:       "Unrecognized end-of-file (EOF)"
	description: """
		Your VRL program contains an [EOF](\(urls.eof)) character that unexpectedly ends the program.
		"""
	resolution: """
		Remove the EOF character.
		"""
}
