remap: features: ergonomic_safety: {
	title:       "Ergonomic safety"
	description: """
		VRL is ergonomically safe in that it makes it difficult to create slow or buggy VRL programs.
		While VRL's [compile-time checks](#\(features.compilation.anchor)) prevent runtime errors, they can't prevent
		some of the more elusive performance and maintainability problems that stem from program complexity—problems
		that can result in observability pipeline instability and unexpected resource costs. To protect against these
		more subtle ergonomic problems, VRL is a carefully *limited* language that offers only those features necessary
		to transform observability data. Any features that are extraneous to that task or likely to result in degraded
		ergonomics are omitted from the language by design.
		"""

	principles: {
		performance: true
		safety:      true
	}

	characteristics: {
		internal_logging_limitation: {
			title: "Internal logging limitation"
			description: """
				VRL programs do produce internal logs but not a rate that's bound to saturate I/O.
				"""
		}
		io_limitation: {
			title: "I/O limitation"
			description: """
				VRL lacks access to system I/O, which tends to be computationally expensive, to require careful
				caching, and to produce degraded performance.
				"""
		}
		recursion_limitation: {
			title: "Lack of recursion"
			description: """
				VRL lacks recursion capabilities, making it impossible to create large or infinite loops that could
				stall VRL programs or needlessly drain memory.
				"""
		}
		no_custom_functions: {
			title: "Lack of custom functions"
			description: """
				VRL requires you to use only its built-in functions and doesn't enable you to create your own. This
				keeps VRL programs easy to debug and reason about.
				"""
		}
		state_limitation: {
			title: "Lack of state"
			description: """
				VRL lacks the ability to hold and maintain state across events. This prevents things like unbounded
				memory growth, hard-to-debug production issues, and unexpected program behavior.
				"""
		}
		rate_limited_logging: {
			title:       "Rate-limited logging"
			description: """
				The VRL [`log`](\(urls.vrl_functions)#log) function implements rate limiting by default. This ensures
				that VRL programs invoking the `log` method don't accidentally saturate I/O.
				"""
		}
		purpose_built: {
			title:       "Purpose built for observability"
			description: """
				VRL is laser focused on observability use cases and *only* those use cases. This makes many
				frustration- and complexity-producing constructs you find in other languages completely superfluous.
				Functions like [`parse_syslog`](\(urls.vrl_functions)#parse_syslog) and
				[`parse_key_value`](\(urls.vrl_functions)#parse_key_value), for example, make otherwise complex tasks simple
				and prevent the need for complex low-level constructs.
				"""
		}
	}
}
