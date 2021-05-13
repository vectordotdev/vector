remap: features: native: {
	title:       "Vector & Rust native"
	description: """
		Like Vector, VRL is built with [Rust](\(urls.rust)) and compiles to native Rust code. Therefore, it inherits
		Rust's safety and performance characteristics that make it ideal for observability pipelines. And because both
		VRL and Vector are written in Rust, they are tightly integrated, avoiding communication inefficiencies such as
		event serialization or [foreign function interfaces](\(urls.ffi)) (FFI). This makes VRL significantly faster
		than non-Rust alternatives.
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
