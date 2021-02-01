remap: features: affine_type_system: {
	title:       "Affine type system"
	description: """
		VRL is built with [Rust](\(urls.rust)) and therefore inherits its
		[affine type system](\(urls.affine_type_system)). This makes VRL exceptionally fast, memory-efficient, and
		memory-safe.
		"""

	principles: {
		performance: true
		safety:      false
	}

	characteristics: {
		absence_of_garbage_collection: {
			title: "Absense of garbage collection"
			description: """
				VRL does not include a garbage collector, avoiding the pauses and performance pitfalls associated
				with runtimes that require garbage collection.
				"""
		}
	}
}
