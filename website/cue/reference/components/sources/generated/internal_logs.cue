package metadata

generated: components: sources: internal_logs: configuration: {
	host_key: {
		description: """
			Overrides the name of the log field used to add the current hostname to each event.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			Set to `""` to suppress this key.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: {}
	}
	level: {
		description: """
			The maximum verbosity of log events to expose.

			Log events at this severity level and above are delivered to this source,
			independently of the console log level Vector was started with (`VECTOR_LOG`,
			`--verbose`, and `--quiet`).

			This setting takes effect once the configuration has been loaded. The few events emitted
			before that, during early startup, are captured at the console log level (with a floor of
			`info`), so exposing `debug` or `trace` events from early startup additionally requires
			starting Vector with a verbose console log level (for example, `VECTOR_LOG=debug`).
			"""
		required: false
		type: string: {
			default: "info"
			enum: {
				debug: "Expose log events at the `DEBUG` level and above."
				error: "Expose only log events at the `ERROR` level."
				info:  "Expose log events at the `INFO` level and above."
				trace: "Expose all log events."
				warn:  "Expose log events at the `WARN` level and above."
			}
		}
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
