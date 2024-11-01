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
		auto_generated:   true
		acknowledgements: true
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

	configuration: base.components.sources.file.configuration & {
		remove_after_secs: warnings: [
			"""
				Vector’s process must have permission to delete files.
				""",
		]
	}

	output: logs: line: {
		description: "An individual line from a file. Lines can be merged using the `multiline` options."
		fields: {
			file: {
				description: "The absolute path of originating file."
				required:    true
				type: string: {
					examples: ["\(_directory)/apache/access.log"]
				}
			}
			host: fields._local_host
			message: {
				description: "The raw line from the file."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["file"]
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
			input: _line
			output: log: {
				file:        _file
				host:        _values.local_host
				message:     _line
				source_type: "file"
				timestamp:   _values.current_timestamp
			}
		},
	]

	how_it_works: {
		autodiscovery: {
			title: "Autodiscovery"
			body: """
				Vector will continually look for new files matching any of your
				include patterns. The frequency is controlled via the
				`glob_minimum_cooldown` option. If a new file is added that matches
				any of the supplied patterns, Vector will begin tailing it. Vector
				maintains a unique list of files and will not tail a file more than
				once, even if it matches multiple patterns. You can read more about
				how we identify files in the Identification section.
				"""
		}

		compressed_files: {
			title: "Compressed Files"
			body: """
				Vector will transparently detect files which have been compressed
				using Gzip and decompress them for reading. This detection process
				looks for the unique sequence of bytes in the Gzip header and does
				not rely on the compressed files adhering to any kind of naming
				convention.

				One caveat with reading compressed files is that Vector is not able
				to efficiently seek into them. Rather than implement a
				potentially-expensive full scan as a seek mechanism, Vector
				currently will not attempt to make further reads from a file for
				which it has already stored a checkpoint in a previous run. For
				this reason, users should take care to allow Vector to fully
				process any compressed files before shutting the process down or moving the
				files to another location on disk.
				"""
		}

		file_deletion: {
			title: "File Deletion"
			body: """
				When a watched file is deleted, Vector will maintain its open file
				handle and continue reading until it reaches `EOF`. When a file is
				no longer findable in the `includes` option and the reader has
				reached `EOF`, that file's reader is discarded.
				"""
		}

		file_read_order: {
			title: "File Read Order"
			body: """
				By default, Vector attempts to allocate its read bandwidth fairly
				across all of the files it's currently watching. This prevents a
				single very busy file from starving other independent files from
				being read. In certain situations, however, this can lead to
				interleaved reads from files that should be read one after the
				other.

				For example, consider a service that logs to timestamped file,
				creating a new one at an interval and leaving the old one as-is.
				Under normal operation, Vector would follow writes as they happen to
				each file and there would be no interleaving. In an overload
				situation, however, Vector may pick up and begin tailing newer files
				before catching up to the latest writes from older files. This would
				cause writes from a single logical log stream to be interleaved in
				time and potentially slow down ingestion as a whole, since the fixed
				total read bandwidth is allocated across an increasing number of
				files.

				To address this type of situation, Vector provides the
				`oldest_first` option. When set, Vector will not read from any file
				younger than the oldest file that it hasn't yet caught up to. In
				other words, Vector will continue reading from older files as long
				as there is more data to read. Only once it hits the end will it
				then move on to read from younger files.

				Whether or not to use the oldest_first flag depends on the
				organization of the logs you're configuring Vector to tail. If your
				`include` option contains multiple independent logical log streams
				(e.g. Nginx's access.log and error.log, or logs from multiple
				services), you are likely better off with the default behavior. If
				you're dealing with a single logical log stream or if you value
				per-stream ordering over fairness across streams, consider setting
				the `oldest_first` option to true.
				"""
		}

		file_rotation: {
			title: "File Rotation"
			body: """
				Vector supports tailing across a number of file rotation strategies.
				The default behavior of `logrotate` is simply to move the old log
				file and create a new one. This requires no special configuration of
				Vector, as it will maintain its open file handle to the rotated log
				until it has finished reading and it will find the newly created
				file normally.

				A popular alternative strategy is `copytruncate`, in which
				`logrotate` will copy the old log file to a new location before
				truncating the original. Vector will also handle this well out of
				the box, but there are a couple configuration options that will help
				reduce the very small chance of missed data in some edge cases. We
				recommend a combination of `delaycompress` (if applicable) on the
				`logrotate` side and including the first rotated file in Vector's
				`include` option. This allows Vector to find the file after rotation,
				read it uncompressed to identify it, and then ensure it has all of
				the data, including any written in a gap between Vector's last read
				and the actual rotation event.
				"""
		}

		fingerprint: {
			title: "Fingerprinting"
			body:  """
				By default, Vector identifies files by running a [cyclic redundancy
				check](\(urls.crc)) (CRC) on the first N lines of the file. This serves as a
				*fingerprint* that uniquely identifies the file. The number of lines, N, that are
				read can be set using the [`fingerprint.lines`](#fingerprint.lines) and
				[`fingerprint.ignored_header_bytes`](#fingerprint.ignored_header_bytes) options.

				This strategy avoids the common pitfalls associated with using device and inode
				names since inode names can be reused across files. This enables Vector to properly
				tail files across various rotation strategies.
				"""
		}

		globbing: {
			title: "Globbing"
			body:  """
				[Globbing](\(urls.globbing)) is supported in all provided file paths,
				files will be autodiscovered continually at a rate defined by the
				`glob_minimum_cooldown` option.
				"""
		}

		line_delimiters: {
			title: "Line Delimiters"
			body: """
				Each line is read until a new line delimiter (by default, `\n` i.e.
				the `0xA` byte) or `EOF` is found. If needed, the default line
				delimiter can be overridden via the `line_delimiter` option.
				"""
		}

		multiline_messages: {
			title: "Multiline Messages"
			body: """
				Sometimes a single log event will appear as multiple log lines. To
				handle this, Vector provides a set of `multiline` options. These
				options were carefully thought through and will allow you to solve the
				simplest and most complex cases. Let's look at a few examples:
				"""
			sub_sections: [
				{
					title: "Example 1: Ruby Exceptions"
					body: #"""
						Ruby exceptions, when logged, consist of multiple lines:

						```text
						foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)
							from foobar.rb:6:in `bar'
							from foobar.rb:2:in `foo'
							from foobar.rb:9:in `<main>'
						```

						To consume these lines as a single event, use the following Vector
						configuration:

						```toml
						[sources.my_file_source]
							type = "file"
							# ...

							[sources.my_file_source.multiline]
								start_pattern = '^[^\s]'
								mode = "continue_through"
								condition_pattern = '^[\s]+from'
								timeout_ms = 1000
						```

						* `start_pattern`, set to `^[^\s]`, tells Vector that new
							multi-line events should _not_ start  with white-space.
						* `mode`, set to `continue_through`, tells Vector continue
							aggregating lines until the `condition_pattern` is no longer
							valid (excluding the invalid line).
						* `condition_pattern`, set to `^[\s]+from`, tells Vector to
							continue aggregating lines if they start with white-space
							followed by `from`.
						"""#
				},
				{
					title: "Example 2: Line Continuations"
					body: #"""
						Some programming languages use the backslash (`\`) character to
						signal that a line will continue on the next line:

						```text
						First line\
						second line\
						third line
						```

						To consume these lines as a single event, use the following Vector
						configuration:

						```toml
						[sources.my_file_source]
							type = "file"
							# ...

							[sources.my_file_source.multiline]
								start_pattern = '\\$'
								mode = "continue_past"
								condition_pattern = '\\$'
								timeout_ms = 1000
						```

						* `start_pattern`, set to `\\$`, tells Vector that new multi-line
							events start with lines that end in `\`.
						* `mode`, set to `continue_past`, tells Vector continue
							aggregating lines, plus one additional line, until
							`condition_pattern` is false.
						* `condition_pattern`, set to `\\$`, tells Vector to continue
							aggregating lines if they _end_ with a `\` character.
						"""#
				},
				{
					title: "Example 3: Line Continuations"
					body: #"""
						Activity logs from services such as Elasticsearch typically begin
						with a timestamp, followed by information on the specific
						activity, as in this example:

						```text
						[2015-08-24 11:49:14,389][ INFO ][env                      ] [Letha] using [1] data paths, mounts [[/
						(/dev/disk1)]], net usable_space [34.5gb], net total_space [118.9gb], types [hfs]
						```

						To consume these lines as a single event, use the following Vector
						configuration:

						```toml
						[sources.my_file_source]
						type = "file"
						# ...

						[sources.my_file_source.multiline]
						start_pattern = '^\[[0-9]{4}-[0-9]{2}-[0-9]{2}'
						mode = "halt_before"
						condition_pattern = '^\[[0-9]{4}-[0-9]{2}-[0-9]{2}'
						timeout_ms = 1000
						```

						* `start_pattern`, set to `^\[[0-9]{4}-[0-9]{2}-[0-9]{2}`, tells
							Vector that new multi-line events start with a timestamp
							sequence.
						* `mode`, set to `halt_before`, tells Vector to continue
							aggregating lines as long as the `condition_pattern` does not
							match.
						* `condition_pattern`, set to `^\[[0-9]{4}-[0-9]{2}-[0-9]{2}`,
							tells Vector to continue aggregating up until a line starts with
							a timestamp sequence.
						"""#
				},
			]
		}

		permissions: {
			title: "File permissions"
			body:  """
				To be able to source events from the files, Vector must be able
				to read the files and execute their parent directories.

				If you have deployed Vector as using one our distributed
				packages, then you will find Vector running as the `vector`
				user. You should ensure this user has read access to the desired
				files used as `include`. Strategies for this include:

				* Create a new unix group, make it the group owner of the
				  target files, with read access, and  add `vector` to that
				  group
				* Use [POSIX ACLs](\(urls.posix_acls)) to grant access to the
				  files to the `vector` user
				* Grant the `CAP_DAC_READ_SEARCH` [Linux
				  capability](\(urls.linux_capability)). This capability
				  bypasses the file system permissions checks to allow
				  Vector to read any file. This is not recommended as it gives
				  Vector more permissions than it requires, but it is
				  recommended over running Vector as `root` which would grant it
				  even broader permissions. This can be granted via SystemD by
				  creating an override file using `systemctl edit vector` and
				  adding:

				  ```
				  AmbientCapabilities=CAP_DAC_READ_SEARCH
				  CapabilityBoundingSet=CAP_DAC_READ_SEARCH
				  ```

				On Debian-based distributions, the `vector` user is
				automatically added to the [`adm`
				group](\(urls.debian_system_groups)), if it exists, which has
				permissions to read `/var/log`.
				"""
		}

		read_position: {
			title: "Read Position"
			body: """
				By default, Vector will read from the beginning of newly discovered
				files. You can change this behavior by setting the `read_from` option to
				`"end"`.

				Previously discovered files will be [checkpointed](#checkpointing), and
				the read position will resume from the last checkpoint. To disable this
				behavior, you can set the `ignore_checkpoints` option to `true`.  This
				will cause Vector to disregard existing checkpoints when determining the
				starting read position of a file.
				"""
		}
	}

	telemetry: metrics: {
		checkpoints_total:     components.sources.internal_metrics.output.metrics.checkpoints_total
		checksum_errors_total: components.sources.internal_metrics.output.metrics.checksum_errors_total
		files_added_total:     components.sources.internal_metrics.output.metrics.files_added_total
		files_deleted_total:   components.sources.internal_metrics.output.metrics.files_deleted_total
		files_resumed_total:   components.sources.internal_metrics.output.metrics.files_resumed_total
		files_unwatched_total: components.sources.internal_metrics.output.metrics.files_unwatched_total
	}
}
