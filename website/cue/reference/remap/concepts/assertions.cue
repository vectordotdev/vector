remap: concepts: assertions: {
	title:       "Assertions"
	description: """
		VRL offers two functions that you can use to assert that VRL values conform to your
		expectations: [`assert`](\(urls.vrl_functions)/#assert) and
		[`assert_eq`](\(urls.vrl_functions)/#assert_eq). `assert` aborts the VRL program and logs an
		error if the provided [Boolean expression](#boolean-expressions) evaluates to `false`, while
		`assert_eq` fails logs an error if the provided values aren't equal. Both functions also
		enable you to provide custom log messages to be emitted upon failure.

		When running Vector, assertions can be useful in situations where you need to be notified
		when any observability event fails a condition. When writing [unit
		tests](\(urls.vector_unit_tests)), assertions can provide granular insight into which
		test conditions have failed and why.
		"""
}
