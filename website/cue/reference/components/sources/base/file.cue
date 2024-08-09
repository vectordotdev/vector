package metadata

base: components: sources: file: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: "Whether or not end-to-end acknowledgements are enabled for this source."
			required:    false
			type: bool: {}
		}
	}
	data_dir: {
		description: """
			The directory used to persist file checkpoint positions.

			By default, the [global `data_dir` option][global_data_dir] is used.
			Make sure the running user has write permissions to this directory.

			If this directory is specified, then Vector will attempt to create it.

			[global_data_dir]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
			"""
		required: false
		type: string: examples: ["/var/local/lib/vector/"]
	}
	encoding: {
		description: "Character set encoding."
		required:    false
		type: object: options: charset: {
			description: """
				Encoding of the source messages.

				Takes one of the encoding [label strings](https://encoding.spec.whatwg.org/#concept-encoding-get) defined as
				part of the [Encoding Standard](https://encoding.spec.whatwg.org/).

				When set, the messages are transcoded from the specified encoding to UTF-8, which is the encoding that is
				assumed internally for string-like data. Enable this transcoding operation if you need your data to
				be in UTF-8 for further processing. At the time of transcoding, any malformed sequences (that can't be mapped to
				UTF-8) is replaced with the Unicode [REPLACEMENT
				CHARACTER](https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character) and warnings are
				logged.
				"""
			required: true
			type: string: examples: ["utf-16le", "utf-16be"]
		}
	}
	exclude: {
		description: """
			Array of file patterns to exclude. [Globbing](https://vector.dev/docs/reference/configuration/sources/file/#globbing) is supported.

			Takes precedence over the `include` option. Note: The `exclude` patterns are applied _after_ the attempt to glob everything
			in `include`. This means that all files are first matched by `include` and then filtered by the `exclude`
			patterns. This can be impactful if `include` contains directories with contents that are not accessible.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["/var/log/binary-file.log"]
		}
	}
	file_key: {
		description: """
			Overrides the name of the log field used to add the file path to each event.

			The value is the full path to the file where the event was read message.

			Set to `""` to suppress this key.
			"""
		required: false
		type: string: {
			default: "file"
			examples: [
				"path",
			]
		}
	}
	fingerprint: {
		description: """
			Configuration for how files should be identified.

			This is important for `checkpointing` when file rotation is used.
			"""
		required: false
		type: object: options: {
			ignored_header_bytes: {
				description: """
					The number of bytes to skip ahead (or ignore) when reading the data used for generating the checksum.

					This can be helpful if all files share a common header that should be skipped.
					"""
				relevant_when: "strategy = \"checksum\""
				required:      false
				type: uint: {
					default: 0
					unit:    "bytes"
				}
			}
			lines: {
				description: """
					The number of lines to read for generating the checksum.

					If your files share a common header that is not always a fixed size,

					If the file has less than this amount of lines, it won’t be read at all.
					"""
				relevant_when: "strategy = \"checksum\""
				required:      false
				type: uint: {
					default: 1
					unit:    "lines"
				}
			}
			strategy: {
				description: """
					The strategy used to uniquely identify files.

					This is important for checkpointing when file rotation is used.
					"""
				required: false
				type: string: {
					default: "checksum"
					enum: {
						checksum: "Read lines from the beginning of the file and compute a checksum over them."
						device_and_inode: """
															Use the [device and inode][inode] as the identifier.

															[inode]: https://en.wikipedia.org/wiki/Inode
															"""
					}
				}
			}
		}
	}
	glob_minimum_cooldown_ms: {
		description: """
			The delay between file discovery calls.

			This controls the interval at which files are searched. A higher value results in greater
			chances of some short-lived files being missed between searches, but a lower value increases
			the performance impact of file discovery.
			"""
		required: false
		type: uint: {
			default: 1000
			unit:    "milliseconds"
		}
	}
	host_key: {
		description: """
			Overrides the name of the log field used to add the current hostname to each event.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			Set to `""` to suppress this key.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: examples: ["hostname"]
	}
	ignore_checkpoints: {
		description: """
			Whether or not to ignore existing checkpoints when determining where to start reading a file.

			Checkpoints are still written normally.
			"""
		required: false
		type: bool: {}
	}
	ignore_not_found: {
		description: """
			Ignore missing files when fingerprinting.

			This may be useful when used with source directories containing dangling symlinks.
			"""
		required: false
		type: bool: default: false
	}
	ignore_older_secs: {
		description: "Ignore files with a data modification date older than the specified number of seconds."
		required:    false
		type: uint: {
			examples: [
				600,
			]
			unit: "seconds"
		}
	}
	include: {
		description: "Array of file patterns to include. [Globbing](https://vector.dev/docs/reference/configuration/sources/file/#globbing) is supported."
		required:    true
		type: array: items: type: string: examples: ["/var/log/**/*.log"]
	}
	internal_metrics: {
		description: "Configuration of internal metrics for file-based components."
		required:    false
		type: object: options: include_file_tag: {
			description: """
				Whether or not to include the "file" tag on the component's corresponding internal metrics.

				This is useful for distinguishing between different files while monitoring. However, the tag's
				cardinality is unbounded.
				"""
			required: false
			type: bool: default: false
		}
	}
	line_delimiter: {
		description: "String sequence used to separate one file line from another."
		required:    false
		type: string: {
			default: "\n"
			examples: [
				"\r\n",
			]
		}
	}
	max_line_bytes: {
		description: """
			The maximum size of a line before it is discarded.

			This protects against malformed lines or tailing incorrect files.
			"""
		required: false
		type: uint: {
			default: 102400
			unit:    "bytes"
		}
	}
	max_read_bytes: {
		description: """
			Max amount of bytes to read from a single file before switching over to the next file.
			**Note:** This does not apply when `oldest_first` is `true`.

			This allows distributing the reads more or less evenly across
			the files.
			"""
		required: false
		type: uint: {
			default: 2048
			unit:    "bytes"
		}
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
	offset_key: {
		description: """
			Enables adding the file offset to each event and sets the name of the log field used.

			The value is the byte offset of the start of the line within the file.

			Off by default, the offset is only added to the event if this is set.
			"""
		required: false
		type: string: examples: [
			"offset",
		]
	}
	oldest_first: {
		description: "Instead of balancing read capacity fairly across all watched files, prioritize draining the oldest files before moving on to read data from more recent files."
		required:    false
		type: bool: default: false
	}
	read_from: {
		description: "File position to use when reading a new file."
		required:    false
		type: string: {
			default: "beginning"
			enum: {
				beginning: "Read from the beginning of the file."
				end:       "Start reading from the current end of the file."
			}
		}
	}
	remove_after_secs: {
		description: """
			After reaching EOF, the number of seconds to wait before removing the file, unless new data is written.

			If not specified, files are not removed.
			"""
		required: false
		type: uint: {
			examples: [0, 5, 60]
			unit: "seconds"
		}
	}
	rotate_wait_secs: {
		description: """
			How long to keep an open handle to a rotated log file.
			The default value represents "no limit"
			"""
		required: false
		type: uint: {
			default: 9223372036854775807
			unit:    "seconds"
		}
	}
}
