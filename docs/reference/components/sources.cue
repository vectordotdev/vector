package metadata

components: sources: [Name=string]: {
	kind: "source"

	classes: {
		// The behavior function for this source. This is used as a filter to help
		// users find components that serve a function.
		function: "collect" | "receive" | "test"
	}

	features: {
		checkpoint: enabled: bool
		multiline: enabled:  bool
	}

	configuration: {
		if features.checkpoint.enabled {
			data_dir: {
				common:      false
				description: "The directory used to persist file checkpoint positions. By default, the [global `data_dir` option][docs.global-options#data_dir] is used. Please make sure the Vector project has write permissions to this dir."
				required:    false
				type: string: {
					default: null
					examples: ["/var/lib/vector"]
				}
			}
		}

		if features.multiline.enabled {
			multiline: {
				common:      false
				description: "Multiline parsing configuration. If not specified, multiline parsing is disabled."
				required:    false
				type: object: options: {
					condition_pattern: {
						description: "Condition regex pattern to look for. Exact behavior is configured via `mode`."
						required:    true
						sort:        3
						type: string: examples: ["^[\\s]+", "\\\\$", "^(INFO|ERROR) ", ";$"]
					}
					mode: {
						description: "Mode of operation, specifies how the `condition_pattern` is interpreted."
						required:    true
						sort:        2
						type: string: enum: {
							continue_through: "All consecutive lines matching this pattern are included in the group. The first line (the line that matched the start pattern) does not need to match the `ContinueThrough` pattern. This is useful in cases such as a Java stack trace, where some indicator in the line (such as leading whitespace) indicates that it is an extension of the preceding line."
							continue_past:    "All consecutive lines matching this pattern, plus one additional line, are included in the group. This is useful in cases where a log message ends with a continuation marker, such as a backslash, indicating that the following line is part of the same message."
							halt_before:      "All consecutive lines not matching this pattern are included in the group. This is useful where a log line contains a marker indicating that it begins a new message."
							halt_with:        "All consecutive lines, up to and including the first line matching this pattern, are included in the group. This is useful where a log line ends with a termination marker, such as a semicolon."
						}
					}
					start_pattern: {
						description: "Start regex pattern to look for as a beginning of the message."
						required:    true
						sort:        1
						type: string: examples: ["^[^\\s]", "\\\\$", "^(INFO|ERROR) ", "[^;]$"]
					}
					timeout_ms: {
						description: "The maximum time to wait for the continuation. Once this timeout is reached, the buffered message is guaranteed to be flushed, even if incomplete."
						required:    true
						sort:        4
						type: uint: {
							examples: [1_000, 600_000]
							unit: "milliseconds"
						}
					}
				}
			}
		}
	}

	output: {
		logs?: [Name=string]: {
			fields: {
				_host: {
					description: "The local hostname, equivalent to the `gethostname` command."
					required:    true
					type: string: examples: ["host.mydomain.com"]
				}

				_timestamp: {
					description: "The exact time the event was ingested into Vector."
					required:    true
					type: timestamp: {}
				}
			}
		}
	}

	// Example uses for the component.
	examples: {
		log: [
			...{
				input: string
			},
		]
	}

	how_it_works: {
		if features.checkpoint.enabled {
			checkpointing: {
				title: "Checkpointing"
				body: #"""
					Vector checkpoints the current read position after each
					successful read. This ensures that Vector resumes where it left
					off if restarted, preventing data from being read twice. The
					checkpoint positions are stored in the data directory which is
					specified via the global `data_dir` option, but can be overridden
					via the `data_dir` option in the file source directly.
					"""#
			}
		}

		context: {
			title: "Context"
			body: #"""
				By default, the `\( Name )` source will augment events with helpful
				context keys as shown in the "Output" section.
				"""#
		}
	}
}
