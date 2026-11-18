remap: features: compilation: {
	title: "Compilation"
	description: """
		VRL programs are compiled to an internal program representation when Vector loads its configuration.
		The VRL runtime, implemented in Rust, executes that representation for each event:

		* Compilation checks types and requires fallible function errors to be handled before events are processed
		* The runtime operates directly on Vector's event data, avoiding per-event foreign function interface (FFI) calls
		* VRL uses Rust's memory management rather than a garbage collector, avoiding garbage collection pauses
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
				all errors thrown by fallible functions are [handled](\(urls.vrl_error_handling)). Fail safety means that
				you need to make explicit decisions about how to handle potentially
				malformed data—a superior alternative to being surprised by such issues when Vector is already handling
				your data in production.
				"""
		}

		type_safety_checks: {
			title: "Type safety checks"
			description: """
				At compile time, Vector performs [type safety](#type-safety) checks to catch runtime
				errors stemming from type mismatches, for example passing an integer to the `parse_syslog` function,
				which can only take a string. VRL essentially forces you to write programs around the assumption that
				every incoming event could be malformed, which provides a strong bulwark against both human error and
				also the many potential consequences of malformed data.
				"""
		}
	}
}
