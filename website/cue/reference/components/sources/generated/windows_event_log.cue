package metadata

generated: components: sources: windows_event_log: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled for this source.

			When enabled, the source will wait for downstream sinks to acknowledge
			receipt of events before updating checkpoints. This provides exactly-once
			delivery guarantees at the cost of potential duplicate events on restart
			if acknowledgements are pending.

			When disabled (default), checkpoints are updated immediately after reading
			events, which may result in data loss if Vector crashes before events are
			delivered to sinks.
			"""
		required: false
		type: object: options: enabled: {
			description: "Whether or not end-to-end acknowledgements are enabled for this source."
			required:    false
			type: bool: {}
		}
	}
	batch_size: {
		description: """
			Batch size for event processing.

			This controls how many events are processed in a single batch.
			"""
		required: false
		type: uint: {
			default: 100
			examples: [10, 100]
		}
	}
	channels: {
		description: """
			A comma-separated list of channels to read from.

			Common channels include "System", "Application", "Security", "Windows PowerShell".
			Use Windows Event Viewer to discover available channels.
			"""
		required: true
		type: array: items: type: string: examples: ["System,Application,Security", "System"]
	}
	checkpoint_interval_secs: {
		description: """
			Interval in seconds between periodic checkpoint flushes.

			Controls how often bookmarks are persisted to disk in synchronous mode.
			Lower values reduce the window of events that may be re-processed after
			a crash, at the cost of more frequent disk writes.
			"""
		required: false
		type: uint: {
			default: 5
			examples: [5, 1, 30]
		}
	}
	connection_timeout_secs: {
		description: """
			Connection timeout in seconds for event subscription.

			This controls how long to wait for event subscription connection.
			"""
		required: false
		type: uint: {
			default: 30
			examples: [30, 60]
		}
	}
	data_dir: {
		description: """
			The directory where checkpoint data is stored.

			By default, the [global `data_dir` option][global_data_dir] is used.
			Make sure the running user has write permissions to this directory.

			[global_data_dir]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
			"""
		required: false
		type: string: examples: ["/var/lib/vector", "C:\\ProgramData\\vector"]
	}
	event_data_format: {
		description: """
			Custom event data formatting options.

			Maps event field names to custom formatting options.
			"""
		required: false
		type: object: options: "*": {
			description: "An individual event data format override."
			required:    true
			type: string: enum: {
				auto: """
					Keep the original format unchanged (passthrough).
					The field value will not be converted or modified.
					"""
				boolean: """
					Parse and format the field value as a boolean.
					Recognizes "true", "1", "yes", "on" as true (case-insensitive).
					"""
				float:   "Parse and format the field value as a floating-point number."
				integer: "Parse and format the field value as an integer."
				string:  "Format the field value as a string."
			}
		}
	}
	event_query: {
		description: """
			The XPath query for filtering events.

			Allows filtering events using XML Path Language queries.
			If not specified, all events from the specified channels will be collected.
			"""
		required: false
		type: string: examples: ["*[System[Level=1 or Level=2 or Level=3]]", "*[System[(Level=1 or Level=2 or Level=3) and TimeCreated[timediff(@SystemTime) <= 86400000]]]"]
	}
	event_timeout_ms: {
		description: """
			Timeout in milliseconds for waiting for new events.

			Controls the maximum time `WaitForMultipleObjects` blocks before
			returning to check for shutdown signals. Lower values increase
			shutdown responsiveness at the cost of more frequent wake-ups.
			"""
		required: false
		type: uint: {
			default: 5000
			examples: [5000, 10000]
		}
	}
	events_per_second: {
		description: """
			Maximum number of events to process per second.

			When set to a non-zero value, Vector will rate-limit event processing
			to prevent overwhelming downstream systems. A value of 0 (default) means
			no rate limiting is applied.
			"""
		required: false
		type: uint: {
			default: 0
			examples: [100, 1000, 5000]
		}
	}
	field_filter: {
		description: """
			Event field inclusion/exclusion patterns.

			Controls which event fields are included in the output.
			"""
		required: false
		type: object: options: {
			exclude_fields: {
				description: """
					Fields to exclude from the output.

					These fields will be removed from the event data.
					"""
				required: false
				type: array: items: type: string: {}
			}
			include_event_data: {
				description: """
					Whether to include event data fields.

					Event data fields contain application-specific data.
					"""
				required: false
				type: bool: default: true
			}
			include_fields: {
				description: """
					Fields to include in the output.

					If specified, only these fields will be included.
					"""
				required: false
				type: array: items: type: string: {}
			}
			include_system_fields: {
				description: """
					Whether to include system fields.

					System fields include metadata like Computer, TimeCreated, etc.
					"""
				required: false
				type: bool: default: true
			}
			include_user_data: {
				description: """
					Whether to include user data fields.

					User data fields contain additional custom data.
					"""
				required: false
				type: bool: default: true
			}
		}
	}
	ignore_event_ids: {
		description: """
			Ignore specific event IDs.

			Events with these IDs will be filtered out and not sent downstream.
			"""
		required: false
		type: array: {
			default: []
			items: type: uint: examples: [4624, 4625, 4634]
		}
	}
	include_xml: {
		description: """
			Whether to include raw XML data in the output.

			When enabled, the raw XML representation of the event is included
			in the `xml` field of the output event.
			"""
		required: false
		type: bool: default: false
	}
	max_event_age_secs: {
		description: """
			Maximum age of events to process (in seconds).

			Events older than this value will be ignored. If not specified,
			all events will be processed regardless of age.
			"""
		required: false
		type: uint: examples: [86400, 604800]
	}
	max_event_data_length: {
		description: """
			Maximum length for event data field values.

			Event data values longer than this will be truncated with "...\\[truncated\\]" appended.
			Set to 0 for no limit.
			"""
		required: false
		type: uint: {
			default: 0
			examples: [1024, 4096]
		}
	}
	only_event_ids: {
		description: """
			Only include specific event IDs.

			If specified, only events with these IDs will be processed.
			Takes precedence over `ignore_event_ids`.
			"""
		required: false
		type: array: items: type: uint: examples: [1000, 1001, 1002]
	}
	read_existing_events: {
		description: """
			Whether to read existing events or only new events.

			When set to `true`, the source will read all existing events from the channels.
			When set to `false` (default), only new events will be read.
			"""
		required: false
		type: bool: default: false
	}
	render_message: {
		description: """
			Whether to render human-readable event messages.

			When enabled (default), Vector will use the Windows EvtFormatMessage API
			to render localized, human-readable event messages with parameter
			substitution. This matches the behavior of Windows Event Viewer.

			Provider DLL handles are cached per provider, so the performance cost
			is limited to the first event from each provider. Disable only if you
			do not need rendered messages and want to eliminate the DLL loads entirely.
			"""
		required: false
		type: bool: default: true
	}
}
