package metadata

components: sources: windows_event_log: {
	title: "Windows Event Log"

	description: """
		Collects log events from Windows Event Log channels using the native
		Windows Event Log API.
		"""

	classes: {
		delivery: "at_least_once"
		deployment_roles: ["daemon"]
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		collect: {
			checkpoint: enabled: true
			from: service: {
				name:     "Windows Event Log"
				thing:    "Windows Event Log channels"
				url:      "https://learn.microsoft.com/en-us/windows/win32/wes/windows-event-log"
				versions: null
			}
		}
		multiline: enabled: false
	}

	support: {
		requirements: [
			"""
				This source is only supported on Windows. Attempting to use it on
				other operating systems will result in an error at startup.
				""",
		]
		warnings: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.windows_event_log.configuration

	output: {
		logs: event: {
			description: "An individual Windows Event Log event."
			fields: {
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["windows_event_log"]
					}
				}
				timestamp: {
					description: "The timestamp of the event."
					required:    false
					type: timestamp: {}
				}
				message: {
					description: "The rendered event message."
					required:    false
					type: string: {
						examples: ["The service was started successfully."]
					}
				}
				channel: {
					description: "The event log channel name."
					required:    false
					type: string: {
						examples: ["System", "Application", "Security"]
					}
				}
				event_id: {
					description: "The event identifier."
					required:    false
					type: uint: {
						examples: [7036, 4624, 1000]
					}
				}
				provider_name: {
					description: "The name of the event provider."
					required:    false
					type: string: {
						examples: ["Microsoft-Windows-Security-Auditing"]
					}
				}
				computer: {
					description: "The name of the computer that generated the event."
					required:    false
					type: string: {
						examples: ["DESKTOP-ABC123"]
					}
				}
				level: {
					description: "The event severity level."
					required:    false
					type: string: {
						examples: ["Information", "Warning", "Error", "Critical"]
					}
				}
			}
		}
	}

	how_it_works: {
		read_existing_events: {
			title: "Reading existing events"
			body:  """
				With the default `read_existing_events = false`, the source has been observed to
				subscribe to all channels without error but never deliver an event and never
				write a checkpoint file, while new events were demonstrably being written to
				those channels. Nothing in Vector's own log points to the problem. See
				[issue #26117](\(urls.vector_issues)/26117).

				Setting `read_existing_events = true` avoids this. The read position is
				persisted per channel in the data directory, so the existing backlog is read
				once on the first start and not again after a restart.
				"""
		}
		query_complexity: {
			title: "Query complexity limit"
			body: """
				Windows limits how many expressions a structured XPath query may contain. A query
				that exceeds the limit is rejected when the subscription is created, with
				`ERROR_EVT_INVALID_QUERY` (`0x80073A99`), and the source does not start. Every
				comparison such as `EventID=4624` or `Level=2` counts as one expression. In
				practice the limit is about 20; a query with 26 `EventID=` comparisons was
				rejected.

				`only_event_ids` is translated into one `EventID=` comparison per listed ID, so a
				longer list runs into the same limit. Use `event_query` with ranges instead, where
				a range costs two expressions regardless of its width:

				```toml
				event_query = "*[System[(EventID=4624 or EventID=4625 or (EventID>=4720 and EventID<=4733))]]"
				```
				"""
		}
		one_source_per_channel: {
			title: "One source per channel"
			body: """
				Each channel is an exclusive resource within a configuration. Two
				`windows_event_log` sources that list the same channel fail at startup with
				``Resource `disk buffer "Security"` is claimed by multiple components``. Splitting
				a channel across two sources, for example to work around the query complexity
				limit, is therefore not possible; combine the conditions into a single
				`event_query` instead.
				"""
		}
		gelf_encoding: {
			title: "Encoding events as GELF"
			body: """
				Several fields of this source do not fit the GELF encoder, which rejects nested
				values and reserves the `level` and `version` fields:

				* `event_data` and `user_data` are objects.
				* `string_inserts` and `keyword_names` are arrays.
				* `level` is a string (`"Error"`), while GELF expects a numeric syslog severity.
				* `version` is the event's integer version, while GELF reserves `version` for its
				  own protocol version.

				Such events fail to serialize at the sink and are dropped. A `remap` transform in
				front of the sink can make them valid:

				```coffee
				. = flatten!(., "_")
				. = map_values(.) -> |v| { if is_array(v) { encode_json(v) } else { v } }
				.win_level = del(.level)
				lvl = to_int(.level_value) ?? 4
				.level = if lvl == 1 { 2 } else if lvl == 2 { 3 } else if lvl == 3 { 4 } else { 6 }
				if exists(.version) { .win_version = del(.version) }
				```

				The values of `keyword_names` are rendered in the display language of the host
				(for example `Classic` or `Klassisch`), so filters should not depend on them. The
				`level` field is not localized.
				"""
		}
	}
}
