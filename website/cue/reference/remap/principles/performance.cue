remap: principles: performance: {
	title:       "Performance"
	description: """
		VRL is implemented in [Rust](\(urls.rust)). Programs are compiled to an internal representation when
		Vector loads its configuration and executed by the VRL runtime for each event. Compile-time correctness
		checks catch errors before events are processed, while the absence of unbounded loops and user-defined
		recursion limits the ways programs can accidentally run indefinitely. Collection iteration is supported
		through built-in functions such as [`for_each`](\(urls.vrl_functions)#for_each).
		"""
}
