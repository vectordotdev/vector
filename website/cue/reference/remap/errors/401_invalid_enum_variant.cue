package metadata

remap: errors: "401": {
	title: "Invalid enum variant"
	description: """
		VRL expects an enum value for this argument, but the value you entered for the enum is
		invalid.
		"""
	resolution: """
		Check the documentation for this function in the [VRL functions
		reference](\(urls.vrl_functions)) to see which enum values are valid for this argument.
		"""
}
