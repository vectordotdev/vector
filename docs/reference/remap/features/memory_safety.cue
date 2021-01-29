remap: features: memory_safety: {
	title:       "Memory safety"
	description: """
		VRL is [memory-safe](\(urls.memory_safety)), protected from various software bugs and security vulnerabilities
		that deal with memory access. This makes VRL ideal for infrastructure use cases, like observability pipelines,
		where reliability and security are top concerns.
		"""

	principles: {
		performance: false
		safety:      true
	}
}
