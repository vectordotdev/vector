remap: features: native: {
	title:       "Vector & Rust native"
	description: """
		Like Vector, VRL is implemented in [Rust](\(urls.rust)). Its runtime executes programs within Vector,
		allowing both to share event data without per-event serialization or calls through a
		[foreign function interface](\(urls.ffi)) (FFI).
		"""

	principles: {
		performance: true
		safety:      true
	}

	characteristics: {
		lack_of_gc: {
			title:       "Lack of garbage collection"
			description: """
				Rust's [affine type system](\(urls.affine_type_system)) avoids the need for garbage collection, making
				VRL exceptionally fast, memory efficient, and memory safe. Memory is precisely allocated and freed,
				avoiding the pauses and performance pitfalls associated with garbage collectors.
				"""
		}
	}
}
