package metadata

components: sources: exec: {
	title: "Exec"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["sidecar"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: false
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "`newline_delimited` for codecs other than `native`, which defaults to `length_delimited`"
		}
		receive: {
			from: {
				service: services.exec
			}

			tls: enabled: false
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.exec.configuration

	output: logs: line: {
		description: "An individual event from exec."
		fields: {
			host:      fields._local_host
			message:   fields._raw_line
			timestamp: fields._current_timestamp
			data_stream: {
				common:      true
				description: "The data stream from which the event originated."
				required:    false
				type: string: {
					default: null
					examples: ["stdout", "stderr"]
				}
			}
			pid: {
				description: "The process ID of the command."
				required:    true
				type: uint: {
					examples: [60085, 668]
					unit: null
				}
			}
			command: {
				required:    true
				description: "The command that was run to generate this event."
				type: array: {
					items: type: string: {
						examples: ["echo", "Hello World!", "ls", "-la"]
					}
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["exec"]
				}
			}
		}
	}

	examples: [
		{
			_line:      "64 bytes from 127.0.0.1: icmp_seq=0 ttl=64 time=0.060 ms"
			_timestamp: "2020-03-13T20:45:38.119Z"
			title:      "Exec line"
			configuration: {}
			input: _line
			output: log: {
				data_stream: "stdout"
				pid:         5678
				timestamp:   _timestamp
				host:        _values.local_host
				message:     _line
				source_type: "exec"
			}
		},
	]

	how_it_works: {
		line_delimiters: {
			title: "Line Delimiters"
			body: """
				Each line is read until a new line delimiter, the `0xA` byte, is found or the end of the
				[`maximum_buffer_size_bytes`](#maximum_buffer_size_bytes) is reached.
				"""
		}
		shutdown: {
			title: "Shutting Down"
			body: """
				When Vector begins shutting down (typically due to a SIGTERM), this source will
				signal to the child process to terminate, if it is running, to shut down.

				On *nix platforms, Vector will issue a SIGTERM to the child process, allowing it to
				gracefully shutdown, and the source will continue reading until the process exits or
				Vector's shutdown grace period expires. The duration of the grace period can be
				configured using `--graceful-shutdown-limit-secs`.

				On Windows, the subprocess will be issued a SIGKILL and terminate abruptly. In the
				future we hope to support graceful shutdown of Windows processes as well.
				"""
		}
	}

	telemetry: metrics: {
		command_executed_total:             components.sources.internal_metrics.output.metrics.command_executed_total
		command_execution_duration_seconds: components.sources.internal_metrics.output.metrics.command_execution_duration_seconds
	}
}
