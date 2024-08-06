package metadata

base: components: sources: docker_logs: configuration: {
	auto_partial_merge: {
		description: "Enables automatic merging of partial events."
		required:    false
		type: bool: default: true
	}
	docker_host: {
		description: """
			Docker host to connect to.

			Use an HTTPS URL to enable TLS encryption.

			If absent, the `DOCKER_HOST` environment variable is used. If `DOCKER_HOST` is also absent,
			the default Docker local socket (`/var/run/docker.sock` on Unix platforms,
			`//./pipe/docker_engine` on Windows) is used.
			"""
		required: false
		type: string: examples: ["http://localhost:2375", "https://localhost:2376", "unix:///var/run/docker.sock", "npipe:////./pipe/docker_engine", "/var/run/docker.sock", "//./pipe/docker_engine"]
	}
	exclude_containers: {
		description: """
			A list of container IDs or names of containers to exclude from log collection.

			Matching is prefix first, so specifying a value of `foo` would match any container named `foo` as well as any
			container whose name started with `foo`. This applies equally whether matching container IDs or names.

			By default, the source collects logs for all containers. If `exclude_containers` is configured, any
			container that matches a configured exclusion is excluded even if it is also included with
			`include_containers`, so care should be taken when using prefix matches as they cannot be overridden by a
			corresponding entry in `include_containers`, for example, excluding `foo` by attempting to include `foo-specific-id`.

			This can be used in conjunction with `include_containers`.
			"""
		required: false
		type: array: items: type: string: examples: ["exclude_", "exclude_me_0", "ad08cc418cf9"]
	}
	host_key: {
		description: """
			Overrides the name of the log field used to add the current hostname to each event.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: {}
	}
	include_containers: {
		description: """
			A list of container IDs or names of containers to include in log collection.

			Matching is prefix first, so specifying a value of `foo` would match any container named `foo` as well as any
			container whose name started with `foo`. This applies equally whether matching container IDs or names.

			By default, the source collects logs for all containers. If `include_containers` is configured, only
			containers that match a configured inclusion and are also not excluded get matched.

			This can be used in conjunction with `exclude_containers`.
			"""
		required: false
		type: array: items: type: string: examples: ["include_", "include_me_0", "ad08cc418cf9"]
	}
	include_images: {
		description: """
			A list of image names to match against.

			If not provided, all images are included.
			"""
		required: false
		type: array: items: type: string: examples: ["httpd", "redis"]
	}
	include_labels: {
		description: """
			A list of container object labels to match against when filtering running containers.

			Labels should follow the syntax described in the [Docker object labels](https://docs.docker.com/config/labels-custom-metadata/) documentation.
			"""
		required: false
		type: array: items: type: string: examples: ["org.opencontainers.image.vendor=Vector", "com.mycorp.internal.animal=fish"]
	}
	multiline: {
		description: """
			Multiline aggregation configuration.

			If not specified, multiline aggregation is disabled.
			"""
		required: false
		type: object: options: {
			condition_pattern: {
				description: """
					Regular expression pattern that is used to determine whether or not more lines should be read.

					This setting must be configured in conjunction with `mode`.
					"""
				required: true
				type: string: examples: ["^[\\s]+", "\\\\$", "^(INFO|ERROR) ", ";$"]
			}
			mode: {
				description: """
					Aggregation mode.

					This setting must be configured in conjunction with `condition_pattern`.
					"""
				required: true
				type: string: enum: {
					continue_past: """
						All consecutive lines matching this pattern, plus one additional line, are included in the group.

						This is useful in cases where a log message ends with a continuation marker, such as a backslash, indicating
						that the following line is part of the same message.
						"""
					continue_through: """
						All consecutive lines matching this pattern are included in the group.

						The first line (the line that matched the start pattern) does not need to match the `ContinueThrough` pattern.

						This is useful in cases such as a Java stack trace, where some indicator in the line (such as a leading
						whitespace) indicates that it is an extension of the proceeding line.
						"""
					halt_before: """
						All consecutive lines not matching this pattern are included in the group.

						This is useful where a log line contains a marker indicating that it begins a new message.
						"""
					halt_with: """
						All consecutive lines, up to and including the first line matching this pattern, are included in the group.

						This is useful where a log line ends with a termination marker, such as a semicolon.
						"""
				}
			}
			start_pattern: {
				description: "Regular expression pattern that is used to match the start of a new message."
				required:    true
				type: string: examples: ["^[\\s]+", "\\\\$", "^(INFO|ERROR) ", ";$"]
			}
			timeout_ms: {
				description: """
					The maximum amount of time to wait for the next additional line, in milliseconds.

					Once this timeout is reached, the buffered message is guaranteed to be flushed, even if incomplete.
					"""
				required: true
				type: uint: {
					examples: [1000, 600000]
					unit: "milliseconds"
				}
			}
		}
	}
	partial_event_marker_field: {
		description: """
			Overrides the name of the log field used to mark an event as partial.

			If `auto_partial_merge` is disabled, partial events are emitted with a log field, set by this
			configuration value, indicating that the event is not complete.
			"""
		required: false
		type: string: default: "_partial"
	}
	retry_backoff_secs: {
		description: "The amount of time to wait before retrying after an error."
		required:    false
		type: uint: {
			default: 2
			unit:    "seconds"
		}
	}
	tls: {
		description: """
			Configuration of TLS when connecting to the Docker daemon.

			Only relevant when connecting to Docker with an HTTPS URL.

			If not configured, the environment variable `DOCKER_CERT_PATH` is used. If `DOCKER_CERT_PATH` is absent, then` DOCKER_CONFIG` is used. If both environment variables are absent, the certificates in `~/.docker/` are read.
			"""
		required: false
		type: object: options: {
			ca_file: {
				description: "Path to the CA certificate file."
				required:    true
				type: string: {}
			}
			crt_file: {
				description: "Path to the TLS certificate file."
				required:    true
				type: string: {}
			}
			key_file: {
				description: "Path to the TLS key file."
				required:    true
				type: string: {}
			}
		}
	}
}
