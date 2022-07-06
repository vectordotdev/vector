package metadata

remap: errors: "631": {
	title: "Fallible abort message expression"
	description: """
		You've passed a fallible expression as a message to abort.
		"""

	rationale: """
		An expression that you pass to abort needs to be infallible. Otherwise, the abort expression could fail at runtime.
		"""

	resolution: """
		Make the expression infallible, potentially by handling the error, coalescing the error using `??`, or via some other method.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				abort to_syslog_level(0)
				"""#
			diff: #"""
				- abort to_syslog_level(0)
				+ abort to_syslog_level(0) ?? "other"
				"""#
		},
	]
}
