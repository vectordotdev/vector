remap: features: ergonomic_safety: {
	title:       "Ergonomic-safety"
	description: """
		VRL is ergonomically safe, preventing the production of slow and unmaintainable VRL programs. While VRL's
		[compile-time checks](\(features.compilation.anchor)) prevent runtime errors, they do not prevent more
		ellusive problems that result from program complexity, like performance or maintainability problems. These
		are pernicious problems that result in pipeline instability and high cost. To protect against this, VRL is
		*intentionally* designed with the thoughtful ergonomics that come in the form of limitations.
		"""

	principles: {
		performance: true
		safety:      true
	}

	characteristics: {
		internal_logging_limitation: {
			title: "Internal logging limitation"
			description: """
				VRL programs do not produce internal logs that could otherwise saturate I/O.
				"""
		}
		io_limitation: {
			title: "I/O limitation"
			description: """
				VRL lacks access to I/O, an expensive task that requires careful caching implementation that commonly
				contributes to performance problems.
				"""
		}
		recursion_limitation: {
			title: "Lack of recursion"
			description: """
				VRL lacks recursion capabilities, making it impossible to create infinite loops that could stall VRL
				programs. This is one reason why VRL does not allow the ability to define custom functions.
				"""
		}
		state_limitation: {
			title: "Lack of state"
			description: """
				VRL lacks the ability to hold and maintain state across events. This prevents unbounded memory growth,
				hard to debug production issues, and unexpected VRL program behavior.
				"""
		}
		rate_limited_logging: {
			title: "Rate-limiting logging"
			description: """
				The VRL `log` function, by default, implements rate-limiting. This ensures that VRL programs that
				invoke the `log` method do not accidentally saturate I/O.
				"""
		}
		purpose_built: {
			title: "Purpose-built for observability"
			description: """
				Conversely from limitations, VRL goes deep on purpose-built observability use cases, avoiding the
				need for unsafe low-level constructs. Functions like `parse_syslog` and	`to_hive_partition` make
				otherwise complex tasks simple and avoid the need for complex low-level constructs.
				"""
		}
	}
}
