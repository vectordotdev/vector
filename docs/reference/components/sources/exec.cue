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
		multiline: enabled: false
		receive: {
			from: {
				service: services.exec
			}

			tls: enabled: false
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		mode: {
			description: "The type of exec mechanism."
			required:    true
			warnings: []
			type: string: {
				enum: {
					scheduled: "Scheduled exec mechanism."
					streaming: "Streaming exec mechanism."
				}
				syntax: "literal"
			}
		}
		command: {
			common:      false
			required:    false
			description: "The command to be run."
			warnings: []
			type: string: {
				default: null
				examples: ["echo", "./myscript.sh"]
				syntax: "literal"
			}
		}
		arguments: {
			common:      false
			description: "Array of any arguments to pass to the command."
			required:    false
			type: array: {
				default: null
				items: type: string: {
					examples: ["Hello World!"]
					syntax: "literal"
				}
			}
		}
		exec_interval_secs: {
			common:        false
			description:   "The interval in seconds between scheduled command runs."
			relevant_when: "mode = `scheduled`"
			required:      false
			type: uint: {
				default: 60
				unit:    "seconds"
			}
		}
		event_per_line: {
			common:        false
			description:   "Determine if events should be generated per line."
			relevant_when: "mode = `scheduled`"
			required:      false
			type: bool: default: true
		}
		exec_duration_millis_key: {
			category:      "Context"
			common:        false
			description:   "The key name added to each event representing the duration in millis that the command took to run."
			relevant_when: "mode = `scheduled`"
			required:      false
			warnings: []
			type: string: {
				default: "exec_duration_millis"
				syntax:  "literal"
			}
		}
		respawn_on_exit: {
			common:        false
			description:   "Determine if a streaming command should be restarted if it exits."
			relevant_when: "mode = `streaming`"
			required:      false
			type: bool: default: true
		}
		respawn_interval_secs: {
			common:        false
			description:   "The interval in seconds between restarting streaming commands if needed."
			relevant_when: "mode = `streaming`"
			required:      false
			warnings: []
			type: uint: {
				default: 60
				unit:    "seconds"
			}
		}
		current_dir: {
			common:      false
			required:    false
			description: "The directory from within which to run the command."
			warnings: []
			type: string: {
				default: null
				syntax:  "literal"
			}
		}
		include_stderr: {
			common:      false
			description: "Include the output of stderr when generating events."
			required:    false
			type: bool: default: false
		}
		host_key: {
			category:    "Context"
			common:      false
			description: "The key name added to each event representing the current host. This can also be globally set via the [global `host_key` option][docs.reference.configuration.global-options#host_key]."
			required:    false
			warnings: []
			type: string: {
				default: "host"
				syntax:  "literal"
			}
		}

		pid_key: {
			category:    "Context"
			common:      false
			description: "The key name added to each event representing the process ID of the running command."
			required:    false
			warnings: []
			type: string: {
				default: "pid"
				syntax:  "literal"
			}
		}
		exit_status_key: {
			category:    "Context"
			common:      false
			description: "The key name added to each event representing the exit status of a scheduled command."
			required:    false
			warnings: []
			type: string: {
				default: "exit_status"
				syntax:  "literal"
			}
		}
		command_key: {
			category:    "Context"
			common:      false
			description: "The key name added to each event representing the command that was run."
			required:    false
			warnings: []
			type: string: {
				default: null
				syntax:  "literal"
			}
		}
		argument_key: {
			category:    "Context"
			common:      false
			description: "The key name added to each event representing the arguments that were provided to the command."
			required:    false
			warnings: []
			type: string: {
				default: null
				syntax:  "literal"
			}
		}
	}

	output: logs: line: {
		description: "An individual event from exec."
		fields: {
			data_stream: {
				description: "The data stream from which the event originated."
				required:    true
				type: string: {
					examples: ["stdout", "stderr"]
					syntax: "literal"
				}
			}
			exec_duration_millis: {
				common:        false
				description:   "The duration in milliseconds a scheduled command took to complete."
				relevant_when: "mode = `scheduled`"
				required:      false
				type: uint: {
					default: null
					unit:    "milliseconds"
				}
			}
			exit_status: {
				common:        false
				description:   "The exit status of a scheduled command."
				relevant_when: "mode = `scheduled`"
				required:      false
				type: uint: {
					default: null
					unit:    null
				}
			}
			pid: {
				description: "The process ID of the command."
				required:    true
				type: uint: {
					examples: [0, 1]
					unit: null
				}
			}
			command: {
				common:      false
				description: "The command that was run to generate this event."
				required:    false
				type: string: {
					default: null
					examples: ["echo", "./myscript.sh"]
					syntax: "literal"
				}
			}
			arguments: {
				common:      false
				description: "Array of any arguments that were passed to the command."
				required:    false
				type: array: {
					default: null
					items: type: string: {
						examples: ["Hello World!"]
						syntax: "literal"
					}
				}
			}
			host:      fields._local_host
			message:   fields._raw_line
			timestamp: fields._current_timestamp
		}
	}

	examples: [
		{
			_line:      "64 bytes from 127.0.0.1: icmp_seq=0 ttl=64 time=0.060 ms"
			_timestamp: "2020-03-13T20:45:38.119Z"
			title:      "Exec line"
			configuration: {}
			input: """
				```text
				(_message)
				```
				"""
			output: log: {
				data_stream:          "stdout"
				exec_duration_millis: 1500
				exit_status:          0
				timestamp:            _timestamp
				host:                 _values.local_host
				message:              _line
			}
		},
	]

	how_it_works: {
		line_delimiters: {
			title: "Line Delimiters"
			body: """
				Each line is read until a new line delimiter, the `0xA` byte, is found.
				"""
		}
	}

	telemetry: metrics: {
		processed_bytes_total:   components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:  components.sources.internal_metrics.output.metrics.processed_events_total
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
