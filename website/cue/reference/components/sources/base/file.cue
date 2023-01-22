package metadata

base: components: sources: file: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level. Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

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

			By default, the global `data_dir` option is used. Make sure the running user has write permissions to this directory.
			"""
		required: false
		type: string: {}
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
			type: string: {}
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
			items: type: string: {}
		}
	}
	file_key: {
		description: """
			Overrides the name of the log field used to add the file path to each event.

			The value will be the full path to the file where the event was read message.

			By default, `file` is used.
			"""
		required: false
		type: string: default: "file"
	}
	fingerprint: {
		description: """
			Configuration for how files should be identified.

			This is important for `checkpointing` when file rotation is used.
			"""
		required: false
		type: object: options: {
			bytes: {
				description:   "Maximum number of bytes to use, from the lines that are read, for generating the checksum."
				relevant_when: "strategy = \"checksum\""
				required:      false
				type: uint: {}
			}
			ignored_header_bytes: {
				description: """
					The number of bytes to skip ahead (or ignore) when reading the data used for generating the checksum.

					This can be helpful if all files share a common header that should be skipped.
					"""
				relevant_when: "strategy = \"checksum\""
				required:      false
				type: uint: default: 0
			}
			lines: {
				description: """
					The number of lines to read for generating the checksum.

					If your files share a common header that is not always a fixed size,

					If the file has less than this amount of lines, it won’t be read at all.
					"""
				relevant_when: "strategy = \"checksum\""
				required:      false
				type: uint: default: 1
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
			Delay between file discovery calls, in milliseconds.

			This controls the interval at which files are searched. A higher value results in greater chances of some short-lived files being missed between searches, but a lower value increases the performance impact of file discovery.
			"""
		required: false
		type: uint: default: 1000
	}
	host_key: {
		description: """
			Overrides the name of the log field used to add the current hostname to each event.

			The value is the current hostname.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: {}
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
		type: uint: {}
	}
	include: {
		description: "Array of file patterns to include. [Globbing](https://vector.dev/docs/reference/configuration/sources/file/#globbing) is supported."
		required:    true
		type: array: items: type: string: {}
	}
	line_delimiter: {
		description: "String sequence used to separate one file line from another."
		required:    false
		type: string: default: "\n"
	}
	max_line_bytes: {
		description: """
			The maximum number of bytes a line can contain before being discarded.

			This protects against malformed lines or tailing incorrect files.
			"""
		required: false
		type: uint: default: 102400
	}
	max_read_bytes: {
		description: "An approximate limit on the amount of data read from a single file at a given time."
		required:    false
		type: uint: default: 2048
	}
	message_start_indicator: {
		description: """
			String value used to identify the start of a multi-line message.

			DEPRECATED: This is a deprecated option -- replaced by `multiline` -- and should be removed.
			"""
		required: false
		type: string: {}
	}
	multi_line_timeout: {
		description: """
			How long to wait for more data when aggregating a multi-line message, in milliseconds.

			DEPRECATED: This is a deprecated option -- replaced by `multiline` -- and should be removed.
			"""
		required: false
		type: uint: default: 1000
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
				type: string: {}
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

						This is useful in cases such as a Java stack trace, where some indicator in the line (such as leading
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
				type: string: {}
			}
			timeout_ms: {
				description: """
					The maximum amount of time to wait for the next additional line, in milliseconds.

					Once this timeout is reached, the buffered message is guaranteed to be flushed, even if incomplete.
					"""
				required: true
				type: uint: {}
			}
		}
	}
	offset_key: {
		description: """
			Enables adding the file offset to each event and sets the name of the log field used.

			The value will be the byte offset of the start of the line within the file.

			Off by default, the offset is only added to the event if this is set.
			"""
		required: false
		type: string: {}
	}
	oldest_first: {
		description: "Instead of balancing read capacity fairly across all watched files, prioritize draining the oldest files before moving on to read data from younger files."
		required:    false
		type: bool: default: false
	}
	read_from: {
		description: "File position to use when reading a new file."
		required:    false
		type: string: enum: {
			beginning: "Read from the beginning of the file."
			end:       "Start reading from the current end of the file."
		}
	}
	remove_after_secs: {
		description: """
			Timeout from reaching `EOF` after which file will be removed from filesystem, unless new data is written in the meantime.

			If not specified, files will not be removed.
			"""
		required: false
		type: uint: {}
	}
	start_at_beginning: {
		description: """
			Whether or not to start reading from the beginning of a new file.

			DEPRECATED: This is a deprecated option -- replaced by `ignore_checkpoints`/`read_from` -- and should be removed.
			"""
		required: false
		type: bool: {}
	}
}
