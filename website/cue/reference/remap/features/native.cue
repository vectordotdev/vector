remap: features: native: {
	title:       "Vector & Rust native"
	description: """
		Like Vector, VRL is built with [Rust](\(urls.rust)). Its programs are compiled to an internal
		representation and executed by native Rust code, bringing Rust's safety and performance characteristics
		to observability pipelines. Because VRL and Vector are both written in Rust, they are tightly integrated,
		avoiding per-event serialization and a [foreign function interface](\(urls.ffi)) (FFI) boundary between
		the two. This integration keeps per-event overhead low and gives VRL a performance advantage over
		alternatives that require those extra conversions or boundary crossings.
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
