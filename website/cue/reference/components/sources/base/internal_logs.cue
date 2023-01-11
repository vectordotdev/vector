package metadata

base: components: sources: internal_logs: configuration: {
	host_key: {
		description: """
			Overrides the name of the log field used to add the current hostname to each event.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			Set to `""` to suppress this key.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: default: "host"
	}
	pid_key: {
		description: """
			Overrides the name of the log field used to add the current process ID to each event.

			By default, `"pid"` is used.

			Set to `""` to suppress this key.
			"""
		required: false
		type: string: default: "pid"
	}
}
