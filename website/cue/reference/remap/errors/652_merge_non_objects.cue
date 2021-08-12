package metadata

remap: errors: "652": {
	title: "Only objects can be merged"
	description: """
		You're attempting to merge two values together but one or both isn't an object.
		"""
	rationale:  """
		Amongst VRL's available types, only objects can be merged together. It's not clear what it
		would mean to merge, for example, an object with a Boolean. Please note, however,
		that some other VRL types do have merge-like operations available:

		* Strings can be [concatenated](\(urls.vrl_expressions)#concatenation) together
		* Arrays can be [appended](\(urls.vrl_functions)#append) to other arrays

		These operations may come in handy if you've used [`merge`](\(urls.vrl_functions)#merge) by
		accident.
		"""
	resolution: """
		Make sure that both values that you're merging are VRL objects. If you're not sure whether
		a value is an object, you can use the [`object`](\(urls.vrl_functions)#object) function to
		check.
		"""
}
