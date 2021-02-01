remap: features: compilation: {
	title: "Compilation"
	description: """
		VRL programs are compiled to native Rust code for safe and efficient runtime performance. This ensures VRL
		programs work as expected, avoiding runtime errors that commonly plague observability pipelines.
		"""

	principles: {
		performance: true
		safety:      true
	}

	characteristics: {
		fail_safety_checks: {
			title:       "Fail-safety checks"
			description: """
				At compile-time, Vector will perform [fail-safety](\(features.fail_safety.anchor)) checks to ensure all
				possible errors are handled. For example, failing to pase a string with the `parse_syslog`
				function. This forces you to [handle errors](\(urls.vrl_error_handling)), explicitly deciding what to
				do in the event of malformed data instead of being surprised by them after deploying Vector.
				"""
		}

		type_safety_checks: {
			title:       "Type-safety checks"
			description: """
				At compile-time, Vector will perform [type-safety](\(features.type_safety.anchor)) checks to catch runtime
				errors due to type mismatches. For example, passing an integer to the `parse_syslog` function. This not
				only protects against human error while writing the program, but also malformed data that deviates from
				expected types. This makes it easy to forgo transformation and route malformed data for easy inspection.
				"""
		}
	}
}
