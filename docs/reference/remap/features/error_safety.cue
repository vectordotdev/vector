remap: features: error_safety: {
	title:       "Error safety"
	description: """
		VRL programs are error-safe, meaning a VRL program will not compile unless all errors are handled. This
		contributes strongly to VRL's safety principle, eliminating unexpected runtime errors that often plague
		production observability pipelines. See the [error reference](\(urls.vrl_errors_reference)) for more info
		on error handling.
		"""

	principles: {
		performance: false
		safety:      true
	}
}
