package metadata

components: transforms: lua: {
	title: "Lua"

	description: """
		Transform events with a full embedded [Lua](\(urls.lua)) 5.4 engine.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		program: {
			runtime: {
				name:    "Lua"
				url:     urls.lua
				version: "5.4"
			}
		}
	}

	support: {
		requirements: []
		warnings: [
			"""
			The `lua` transform is ~60% slower than the [`remap` transform](\(urls.vector_remap_transform)); we
			recommend that you use the `remap` transform whenever possible. The `lua` transform is
			designed solely for edge cases not covered by the `remap` transform and not as a go-to option. If the
			`remap` transform doesn't cover your use case, please [open an issue](\(urls.new_feature_request)) and let
			us know.
			""",
		]
		notices: []
	}

	configuration: base.components.transforms.lua.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: false
	}

	examples: [
		{
			title: "Add, rename, and remove log fields"
			configuration: {
				version: "2"
				hooks: process: """
					function (event, emit)
						-- Add root level field
						event.log.field = "new value"
						-- Add nested field
						event.log.nested.field = "nested value"
						-- Rename field
						event.log.renamed_field = event.log.field_to_rename
						event.log.field_to_rename = nil
						-- Remove fields
						event.log.field_to_remove = nil
						emit(event)
					end
					"""
			}
			input: log: {
				field_to_rename: "old value"
				field_to_remove: "remove me"
			}
			output: log: {
				field: "new value"
				nested: field: "nested value"
				renamed_field: "old value"
			}
		},
		{
			title: "Add, rename, remove metric tags"
			configuration: {
				version: "2"
				hooks: process: """
					function (event, emit)
						-- Add tag
						event.metric.tags.tag = "new value"
						-- Rename tag
						event.metric.tags.renamed_tag = event.log.tag_to_rename
						event.metric.tags.tag_to_rename = nil
						-- Remove tag
						event.metric.tags.tag_to_remove = nil
						emit(event)
					end
					"""
			}
			input: metric: {
				kind: "incremental"
				name: "logins"
				counter: {
					value: 2.0
				}
				tags: {
					tag_to_rename: "old value"
					tag_to_remove: "remove me"
				}
			}
			output: metric: {
				kind: "incremental"
				name: "logins"
				counter: {
					value: 2.0
				}
				tags: {
					tag:         "new value"
					renamed_tag: "old value"
				}
			}
		},
		{
			title: "Drop an event"
			configuration: {
				version: "2"
				hooks: process: """
					function (event, emit)
						-- Drop event entirely by not calling the `emit` function
					end
					"""
			}
			input: log: {
				field_to_rename: "old value"
				field_to_remove: "remove me"
			}
			output: null
		},
		{
			title: "Iterate over log fields"
			configuration: {
				version: "2"
				hooks: process: """
					function (event, emit)
						-- Remove all fields where the value is "-"
						for f, v in pairs(event) do
							if v == "-" then
								event[f] = nil
							end
						end
						emit(event)
					end
					"""
			}
			input: log: {
				value_to_remove: "-"
				value_to_keep:   "keep"
			}
			output: log: {
				value_to_keep: "keep"
			}
		},
		{
			title: "Parse timestamps"
			configuration: {
				version: "2"
				hooks: {
					process: "process"
				}
				source: """
					  timestamp_pattern = "(%d%d%d%d)[-](%d%d)[-](%d%d) (%d%d):(%d%d):(%d%d).?(%d*)"
					  function parse_timestamp(str)
						local year, month, day, hour, min, sec, millis = string.match(str, timestamp_pattern)
						local ms = 0
						if millis and millis ~= "" then
							ms = tonumber(millis)
						end
						return {
							year    = tonumber(year),
							month   = tonumber(month),
							day     = tonumber(day),
							hour    = tonumber(hour),
							min     = tonumber(min),
							sec     = tonumber(sec),
							nanosec = ms * 1000000
						}
					  end
					  function process(event, emit)
						event.log.timestamp = parse_timestamp(event.log.timestamp_string)
						emit(event)
					  end
					"""
			}
			input: log: {
				timestamp_string: "2020-04-07 06:26:02.643"
			}
			output: log: {
				timestamp_string: "2020-04-07 06:26:02.643"
				timestamp:        "2020-04-07 06:26:02.643"
			}
		},
		{
			title: "Count the number of logs"
			configuration: {
				version: "2"
				hooks: {
					init:     "init"
					process:  "process"
					shutdown: "shutdown"
				}
				timers: [
					{interval_seconds: 5, handler: "timer_handler"},
				]
				source: """
					function init()
						count = 0
					end
					function process()
						count = count + 1
					end
					function timer_handler(emit)
						emit(make_counter(count))
						count = 0
					end
					function shutdown(emit)
						emit(make_counter(count))
					end
					function make_counter(value)
						return metric = {
							name = "event_counter",
							kind = "incremental",
							timestamp = os.date("!*t"),
							counter = {
								value = value
							}
						}
					end
					"""
			}
			input: log: {}
			output: metric: {
				kind: "incremental"
				name: "event_counter"
				counter: {
					value: 1.0
				}
				tags: {
					tag:         "new value"
					renamed_tag: "old value"
				}
			}
		},
	]

	how_it_works: {
		event_data_model: {
			title: "Event Data Model"
			body:  """
				The `process` hook takes an `event` as its first argument.
				Events are represented as [tables](\(urls.lua_table)) in Lua
				and follow Vector's data model exactly. Please refer to
				Vector's [data model reference](\(urls.vector_data_model)) for the event
				schema. How Vector's types map to Lua's type are covered below.
				"""
			sub_sections: [
				{
					title: "Type Mappings"
					body:  """
						The correspondence between Vector's [data types](\(urls.vector_log_data_types)) and Lua data type is summarized
						by the following table:

						| Vector Type                                         | Lua Type                        | Comment                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
						|:----------------------------------------------------|:--------------------------------|:-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
						| [`String`](\(urls.vector_log)#strings)       | [`string`](\(urls.lua_string))     |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
						| [`Integer`](\(urls.vector_log)#integers)     | [`integer`](\(urls.lua_integer))   |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
						| [`Float`](\(urls.vector_log)#floats)         | [`number`](\(urls.lua_number))     |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
						| [`Boolean`](\(urls.vector_log)#booleans)     | [`boolean`](\(urls.lua_boolean))   |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
						| [`Timestamp`](\(urls.vector_log)#timestamps) | [`table`](\(urls.lua_table))       | There is no dedicated timestamp type in Lua. Timestamps are represented as tables using the convention defined by [`os.date`](\(urls.lua_os_date)) and [`os.time`](\(urls.lua_os_time)). The table representation of a timestamp contains the fields `year`, `month`, `day`, `hour`, `min`, `sec`, `nanosec`, `yday`, `wday`, and `isdst`. If such a table is passed from Lua to Vector, the fields `yday`, `wday`, and `isdst` can be omitted. In addition to the `os.time` representation, Vector supports sub-second resolution with a `nanosec` field in the table. |
						| [`Null`](\(urls.vector_log)#null-values)     | empty string                    | In Lua setting the value of a table field to `nil` means deletion of this field. In addition, the length operator `#` does not work in the expected way with sequences containing nulls. Because of that `Null` values are encoded as empty strings.                                                                                                                                                                                                                                                                                                               |
						| [`Map`](\(urls.vector_log)#maps)             | [`table`](\(urls.lua_table))       |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
						| [`Array`](\(urls.vector_log)#arrays)         | [`sequence`](\(urls.lua_sequence)) | Sequences are a special case of tables. Indexes start from 1, following the Lua convention.                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
						"""
				},
			]
		}
		learning_lua: {
			title: "Learning Lua"
			body:  """
				In order to write non-trivial transforms in Lua, one has to have
				basic understanding of Lua. Because Lua is an easy to learn
				language, reading a few first chapters of
				[the official book](\(urls.lua_pil)) or consulting
				[the manual](\(urls.lua_manual)) would suffice.
				"""
		}
		search_dirs: {
			title: "Search Directories"
			body:  """
				Vector provides a `search_dirs` option that allows you to specify
				absolute paths that will be searched when using the
				[Lua `require` function](\(urls.lua_require)). If this option is not
				set, the directories of the configuration files will be used instead.
				"""
		}
	}

	telemetry: metrics: {
		lua_memory_used_bytes: components.sources.internal_metrics.output.metrics.lua_memory_used_bytes
	}
}
