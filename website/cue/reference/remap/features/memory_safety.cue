remap: features: memory_safety: {
	title:       "Memory safety"
	description: """
		VRL inherits Rusts's [memory safety](\(urls.memory_safety)) guarantees, protecting you from
		[common software bugs and security vulnerabilities](\(urls.memory_safety_bugs)) that stem from improper memory
		access. This makes VRL ideal for infrastructure use cases, like observability pipelines, where reliability and
		security are top concerns.
		"""

	principles: {
		performance: false
		safety:      true
	}
}
