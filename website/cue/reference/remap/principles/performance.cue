remap: principles: performance: {
	title:       "Performance"
	description: """
		VRL is implemented in the very fast and efficient [Rust](\(urls.rust)) language and
		VRL scripts are compiled into Rust code when Vector is started. This means that you can use VRL to
		transform observability data with a minimal per-event performance penalty vis-à-vis pure Rust. In addition,
		ergonomic features such as compile-time correctness checks and the lack of language constructs like
		loops make it difficult to write scripts that are slow or buggy or require optimization.
		"""
}
