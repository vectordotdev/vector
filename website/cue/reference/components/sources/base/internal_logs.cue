package metadata

base: components: sources: internal_logs: configuration: {
	host_key: {
		description: """
			Overrides the name of the log field used to add the current hostname to each event.

			The value will be the current hostname for wherever Vector is running.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: syntax: "literal"
	}
	pid_key: {
		description: """
			Overrides the name of the log field used to add the current process ID to each event.

			The value will be the current process ID for Vector itself.

			By default, `"pid"` is used.
			"""
		required: false
		type: string: syntax: "literal"
	}
}
