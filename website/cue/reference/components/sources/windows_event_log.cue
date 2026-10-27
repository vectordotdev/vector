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
				Windows rejects structured XPath queries whose boolean operators are nested too
				deeply. Such a query fails when the subscription is created, with
				`ERROR_EVT_INVALID_QUERY` (`0x80073A99`), the channel is skipped, and the source
				does not start. A flat `or` chain nests one level per operand: on Windows 11, a
				chain of 23 terms is accepted and a chain of 24 is rejected. A term may itself be
				a parenthesized group: a range such as `(EventID>=4720 and EventID<=4733)` adds
				one term to the chain plus one level for its `and`, so 22 ranges fit where 23
				single IDs do. Grouping only helps while the total nesting stays shallow; four
				groups of 20 IDs are accepted, five are not.

				`only_event_ids` is currently translated into a flat chain with one `EventID=`
				comparison per listed ID, so a list of more than 23 IDs runs into this limit even
				though the configuration validates. Use `event_query` with ranges instead:

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
