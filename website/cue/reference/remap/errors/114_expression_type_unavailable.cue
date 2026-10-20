package metadata

remap: errors: "114": {
	title: "Expression type unavailable"
	description: """
		The type of an expression cannot be determined at compile time because it depends on a
		feature that is not available in the current build configuration.
		"""
	resolution: """
		Enable the required feature or replace the expression with one whose type can be
		statically determined.
		"""
}
