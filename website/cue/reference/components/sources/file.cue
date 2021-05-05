package metadata

components: sources: file: {
	_directory: "/var/log"

	title: "File"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		deployment_roles: ["daemon", "sidecar"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		collect: {
			checkpoint: enabled: true
			from: {
				service: services.files

				interface: file_system: {
					directory: _directory
				}
			}
		}
		multiline: enabled: true
		encoding: enabled:  true
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
		requirements: [
			"""
				The `vector` process must have the ability to read the files
				listed in `include` and execute any of the parent directories
				for these files. Please see [File
				permissions](#file-permissions) for more details.
				""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		exclude: {
			common:      false
			description: "Array of file patterns to exclude. [Globbing](#globbing) is supported.*Takes precedence over the [`include` option](#include).*"
			required:    false
			type: array: {
				default: null
				items: type: string: {
					examples: ["\(_directory)/binary-file.log"]
					syntax: "literal"
				}
			}
		}
		file_key: {
			category:    "Context"
			common:      false
			description: "The key name added to each event with the full path of the file."
			required:    false
			type: string: {
				default: "file"
				examples: ["file"]
				syntax: "literal"
			}
		}
		fingerprint: {
			common:      false
			description: "Configuration for how the file source should identify files."
			required:    false
			type: object: options: {
				strategy: {
					common:      false
					description: "The strategy used to uniquely identify files. This is important for [checkpointing](#checkpointing) when file rotation is used."
					required:    false
					type: string: {
						default: "checksum"
						enum: {
							checksum:         "Read the first line of the file, skipping the first `ignored_header_bytes` bytes, to uniquely identify files via a checksum."
							device_and_inode: "Uses the [device and inode](\(urls.inode)) to unique identify files."
						}
						syntax: "literal"
					}
				}
				ignored_header_bytes: {
					common:        false
					description:   "The number of bytes to skip ahead (or ignore) when generating a unique fingerprint. This is helpful if all files share a common header."
					relevant_when: "strategy = \"checksum\""
					required:      false
					type: uint: {
						default: 0
						unit:    "bytes"
					}
				}
			}
		}
		glob_minimum_cooldown_ms: {
			common:      false
			description: "Delay between file discovery calls. This controls the interval at which Vector searches for files."
			required:    false
			type: uint: {
				default: 1_000
				unit:    "milliseconds"
			}
		}
		host_key: {
			category:    "Context"
			common:      false
			description: """
				The key name added to each event representing the current host. This can also be globally set via the
				[global `host_key` option](\(urls.vector_configuration)/global-options#host_key).
				"""
			required:    false
			type: string: {
				default: "host"
				syntax:  "literal"
			}
		}
		ignore_not_found: {
			common:      false
			description: "Ignore missing files when fingerprinting. This may be useful when used with source directories containing dangling symlinks."
			required:    false
			type: bool: default: false
		}
		ignore_older_secs: {
			common:      true
			description: "Ignore files with a data modification date older than the specified number of seconds."
			required:    false
			type: uint: {
				default: null
				examples: [60 * 10]
				unit: "seconds"
			}
		}
		include: {
			description: "Array of file patterns to include. [Globbing](#globbing) is supported."
			required:    true
			type: array: items: type: string: {
				examples: ["\(_directory)/**/*.log"]
				syntax: "literal"
			}
		}
		line_delimiter: {
			common:      false
			description: "String sequence used to separate one file line from another"
			required:    false
			type: string: {
				default: "\n"
				examples: ["\r\n"]
				syntax: "literal"
			}
		}
		max_line_bytes: {
			common:      false
			description: "The maximum number of a bytes a line can contain before being discarded. This protects against malformed lines or tailing incorrect files."
			required:    false
			type: uint: {
				default: 102_400
				unit:    "bytes"
			}
		}
		max_read_bytes: {
			category:    "Reading"
			common:      false
			description: "An approximate limit on the amount of data read from a single file at a given time."
			required:    false
			type: uint: {
				default: null
				examples: [2048]
				unit: "bytes"
			}
		}
		oldest_first: {
			category:    "Reading"
			common:      false
			description: "Instead of balancing read capacity fairly across all watched files, prioritize draining the oldest files before moving on to read data from younger files."
			required:    false
			type: bool: default: false
		}
		remove_after_secs: {
			common:      false
			description: "Timeout from reaching `eof` after which file will be removed from filesystem, unless new data is written in the meantime. If not specified, files will not be removed."
			required:    false
			warnings: ["Vector's process must have permission to delete files."]
			type: uint: {
				default: null
				examples: [0, 5, 60]
				unit: "seconds"
			}
		}
		read_from: {
			common:      true
			description: "In the absence of a checkpoint, this setting tells Vector where to start reading files that are present at startup."
			required:    false
			type: string: {
				syntax:  "literal"
				default: "beginning"
				enum: {
					"beginning": "Read from the beginning of the file."
					"end":       "Start reading from the current end of the file."
				}
			}
		}
		ignore_checkpoints: {
			common:      false
			description: "This causes Vector to ignore existing checkpoints when determining where to start reading a file. Checkpoints are still written normally."
			required:    false
			type: bool: default: false
		}
	}

	output: logs: line: {
		description: "An individual line from a file. Lines can be merged using the `multiline` options."
		fields: {
			file: {
				description: "The absolute path of originating file."
				required:    true
				type: string: {
					examples: ["\(_directory)/apache/access.log"]
					syntax: "literal"
				}
			}
			host: fields._local_host
			message: {
				description: "The raw line from the file."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					syntax: "literal"
				}
			}
			timestamp: fields._current_timestamp
		}
	}

	examples: [
		{
			_file: "\(_directory)/apache/access.log"
			_line: "53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"
			title: "Apache Access Log"

			configuration: {
				include: ["\(_directory)/**/*.log"]
			}
			input: """
				```text filename="\(_file)"
				\(_line)
				```
				"""
			output: log: {
				file:      _file
				host:      _values.local_host
				message:   _line
				timestamp: _values.current_timestamp
			}
		},
	]

	telemetry: metrics: {
		events_in_total:               components.sources.internal_metrics.output.metrics.events_in_total
		checkpoint_write_errors_total: components.sources.internal_metrics.output.metrics.checkpoint_write_errors_total
		checkpoints_total:             components.sources.internal_metrics.output.metrics.checkpoints_total
		checksum_errors_total:         components.sources.internal_metrics.output.metrics.checksum_errors_total
		file_delete_errors_total:      components.sources.internal_metrics.output.metrics.file_delete_errors_total
		file_watch_errors_total:       components.sources.internal_metrics.output.metrics.file_watch_errors_total
		files_added_total:             components.sources.internal_metrics.output.metrics.files_added_total
		files_deleted_total:           components.sources.internal_metrics.output.metrics.files_deleted_total
		files_resumed_total:           components.sources.internal_metrics.output.metrics.files_resumed_total
		files_unwatched_total:         components.sources.internal_metrics.output.metrics.files_unwatched_total
		fingerprint_read_errors_total: components.sources.internal_metrics.output.metrics.fingerprint_read_errors_total
		glob_errors_total:             components.sources.internal_metrics.output.metrics.glob_errors_total
	}
}
