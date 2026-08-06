remap: features: compilation: {
	title: "Compilation"
	description: """
		VRL programs are compiled to and run as native Rust code. This has several important implications:

		* VRL programs are extremely fast and efficient, with performance characteristics very close to Rust itself
		* VRL has no runtime and thus imposes no per-event foreign function interface (FFI) or data conversion costs
		* VRL has no garbage collection, which means no GC pauses and no accumulated memory usage across events
		"""

	principles: {
		performance: true
		safety:      true
	}

	characteristics: {
		fail_safety_checks: {
			title:       "Fail safety checks"
			description: """
				At compile time, Vector performs [fail safety](#fail-safety) checks to ensure that
				all errors thrown by fallible functions are [handled](\(urls.vrl_error_handling)). If you fail to pass a
				string to the `parse_syslog` function, for example, the VRL compiler aborts and provides a helpful error
				message. Fail safety means that you need to make explicit decisions about how to handle potentially
				malformed data—a superior alternative to being surprised by such issues when Vector is already handling
				your data in production.
				"""
		}

		type_safety_checks: {
			title: "Type safety checks"
			description: """
				At compile time, Vector performs [type safety](#type-safety)) checks to catch runtime
				errors stemming from type mismatches, for example passing an integer to the `parse_syslog` function,
				which can only take a string. VRL essentially forces you to write programs around the assumption that
				every incoming event could be malformed, which provides a strong bulwark against both human error and
				also the many potential consequences of malformed data.
				"""
		}
	}
}
