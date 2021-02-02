remap: features: fail_safety: {
	title:       "Fail-safety"
	description: """
		VRL programs are [fail-safe](\(urls.fail_safe)), meaning a VRL program will not compile unless all possible
		errors are handled. This eliminates unexpected runtime errors that often plague production observability
		pipelines with data loss and downtime. See the [error reference](\(urls.vrl_errors_reference)) for more
		information on VRL errors.
		"""

	principles: {
		performance: false
		safety:      true
	}
}
