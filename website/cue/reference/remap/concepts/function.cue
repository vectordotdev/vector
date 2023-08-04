remap: concepts: function: {
	title:       "Function"
	description: """
		Like most languages, VRL includes [functions](\(urls.vrl_functions)) that represent named
		procedures designed to accomplish specific tasks. Functions are the highest-level construct
		of reusable code in VRL, which, for the sake of simplicity, doesn't include modules,
		classes, or other complex constructs for organizing functions.
		"""

	characteristics: {
		fallibility: {
			title:       "Fallibility"
			description: """
				Some VRL functions are *fallible*, meaning that they can error. Any potential errors
				thrown by fallible functions must be handled, a requirement enforced at compile
				time.

				This feature of VRL programs, which we call [fail safety](\(urls.vrl_fail_safety)),
				is a defining characteristic of VRL and a primary source of its safety guarantees.
				"""
		}
		deprecation: {
			title: "Deprecation"
			description: """
				VRL functions can be marked as "deprecated". When a function
				is deprecated, a warning will be shown at runtime.

				Suggestions on how to update the VRL program can usually be found in the actual warning and the function documentation.
				"""
		}
	}
}
