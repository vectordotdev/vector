package metadata

remap: expressions: abort: {
	title: "Abort"
	description: """
		An `abort` expression causes the VRL program to terminate, aborting any
		modifications made to the event.
		"""
	return: """
		Does not return a value, simply aborts the program.
		"""

	grammar: {
		source: "abort ~ message?"
		definitions: {
			message: {
				description: """
					`message` is an optional debug message that can be used for diagnostic purposes and is included in a `remap` transform's dropped event metadata.
					"""
			}
		}
	}

	examples: [
		{
			title: "Ignoring invalid events"
			input: log: message: "hello world"
			source: #"""
				if contains(string!(.message), "hello") {
					abort
				}
				.message = "not hello world"
				"""#
			return: message: "hello world"
		},
	]
}
