package metadata

remap: functions: for_keys: {
	category: "Enumerate"
	description: """
		Iterate recursively through an object.

        This function exposes the parent keys and value for
        every key iterated through in the object. The keys include
		the parent keys and the current key in an array, effectively
		the path to the value in the object.

		The function uses the "function closure syntax" to allow reading
		the keys/value combination for each item in the
		object.

		The same scoping rules apply to closure blocks as they do for
		regular blocks, meaning, any variable defined in parent scopes
		are accessible, and mutations to those variables are preserved,
		but any new variables instantiated in the closure block are
		unavailable outside of the block.

		Check out the examples below to learn about the closure syntax.
		"""

	arguments: [
		{
			name:        "value"
			description: "The object to iterate."
			required:    true
			type: ["object"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["null"]
	}
	examples: [
		{
			title: "Validate log fields"
			input: log: {
				event: {category: "authentication", example: "remove me"}
			}
			source: #"""

				for_keys(.) -> |keys, value| {
                    # Check the path to the key.
                    # An enrichment table could be used here
                    # for checking log fields against.
                    if join!(keys, ".") != "event.category" {
                        # Remove key if doesnt match
                        . = remove!(., keys)
                    }
			    }

				"""#
			return: {event: {category: "authentication"}}
		},
	]
}
