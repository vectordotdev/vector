package metadata

components: transforms: lua: {
	title:             "Lua"
	short_description: "Accepts log and metric events and allows you to transform events with a full embedded [Lua][urls.lua] engine."
	long_description:  "Accepts log and metric events and allows you to transform events with a full embedded [Lua][urls.lua] engine."

	classes: {
		commonly_used: true
		function:      "program"
	}

	features: {}

	statuses: {
		development: "beta"
	}

	support: {
		input_types: ["log", "metric"]

		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: [
			#"""
				Vector embeds Lua `5.3`.
				"""#,
		]
	}

	configuration: {
		hooks: {
			description: "Configures hooks handlers."
			groups: ["simple", "inline", "module"]
			required: true
			warnings: []
			type: object: {
				examples: []
				options: {
					init: {
						common:      false
						description: "A function which is called when the first event comes, before calling `hooks.process`"
						groups: ["inline", "module"]
						required: false
						warnings: []
						type: string: {
							default: null
							examples: [
								#"""
                function (emit)
                  count = 0 -- initialize state by setting a global variable
                end
                """#,
								"init",
							]
						}
					}
					process: {
						description: "A function which is called for each incoming event. It can produce new events using `emit` function."
						groups: ["simple", "inline", "module"]
						required: true
						warnings: []
						type: string: {
							examples: [
								#"""
                function (event, emit)
                  event.log.field = "value" -- set value of a field
                  event.log.another_field = nil -- remove field
                  event.log.first, event.log.second = nil, event.log.first -- rename field
                  -- Very important! Emit the processed event.
                  emit(event)
                end
                """#,
								"process",
							]
						}
					}
					shutdown: {
						common:      false
						description: "A function which is called when Vector is stopped. It can produce new events using `emit` function."
						groups: ["inline", "module"]
						required: false
						warnings: []
						type: string: {
							default: null
							examples: [
								#"""
                function (emit)
                  emit {
                    metric = {
                      name = "event_counter",
                      kind = "incremental",
                      timestamp = os.date("!*t"),
                      counter = {
                        value = counter
                      }
                    }
                  }
                end
                """#,
								"shutdown",
							]
						}
					}
				}
			}
		}
		search_dirs: {
			common:      false
			description: "A list of directories to search when loading a Lua file via the `require` function. If not specified, the modules are looked up in the directories of Vector's configs."
			groups: ["module"]
			required: false
			warnings: []
			type: "[string]": {
				default: null
				examples: [["/etc/vector/lua"]]
			}
		}
		source: {
			common:      false
			description: "The source which is evaluated when the transform is created."
			groups: ["inline", "module"]
			required: false
			warnings: []
			type: string: {
				default: null
				examples: [
					#"""
						function init()
						  count = 0
						end

						function process()
						  count = count + 1
						end

						function timer_handler(emit)
						  emit(make_counter(counter))
						  counter = 0
						end

						function shutdown(emit)
						  emit(make_counter(counter))
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
						"""#,
					#"""
						-- external file with hooks and timers defined
						require('custom_module')
						"""#,
				]
			}
		}
		timers: {
			common:      false
			description: "Configures timers which are executed periodically at given interval."
			groups: ["inline", "module"]
			required: false
			warnings: []
			type: object: {
				options: {
					handler: {
						description: "Defines a handler function which is executed periodially at `interval_seconds`. It can produce new events using `emit` function."
						required:    true
						warnings: []
						type: string: {
							examples: ["timer_handler"]
						}
					}
					interval_seconds: {
						description: "Defines the interval at which the timer handler would be executed."
						required:    true
						warnings: []
						type: uint: {
							examples: [1, 10, 30]
							unit: "seconds"
						}
					}
				}
			}
		}
		version: {
			description: "Transform API version. Specifying this version ensures that Vector does not break backward compatibility."
			groups: ["simple", "inline", "module"]
			required: true
			warnings: []
			type: string: {
				enum: {
					"2": "Lua transform API version 2"
				}
			}
		}
	}

	examples: {
		log: [
			{
				title: "Add, rename, & remove fields"
				configuration: {
					hooks: process: #"""
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
						"""#
				}
				input: {
					field_to_rename: "old value"
					field_to_remove: "remove me"
				}
				output: {
					field: "new value"
					nested: field: "nested value"
					renamed_field: "old value"
				}
			},
		]
		metric: [
			{
				title: "Add, rename, remove metric tags"
				configuration: {
					hooks: process: #"""
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
						"""#
				}
				input: {
					counter: {
						value: 2
					}
					tags: {
						tag_to_rename: "old value"
						tag_to_remove: "remove me"
					}
				}
				output: {
					counter: {
						value: 2
					}
					tags: {
						tag:         "new value"
						renamed_tag: "old value"
					}
				}
			},
			{
				title: "Drop metric event"
				configuration: {
					hooks: process: #"""
						function (event, emit)
						  -- Drop event entirely by not calling the `emit` function
						end
						"""#
				}
				input: {
					counter: {
						value: 2
					}
					tags: {
						tag_to_rename: "old value"
						tag_to_remove: "remove me"
					}
				}
				output: null
			},
		]
	}

	how_it_works: {
		defining_timestamps: {
			title: "Defining Timestamps"
			body: #"""
				To parse a timestamp with an optional milliseconds field, like `2020-04-07 06:26:02.643` or `2020-04-07 06:26:02`:

				```lua
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

				parse_timestamp('2020-04-07 06:26:02.643')
				parse_timestamp('2020-04-07 06:26:02')
				```
				"""#
		}
		learning_lua: {
			title: "Learning Lua"
			body: #"""
				In order to write non-trivial transforms in Lua, one has to have
				basic understanding of Lua. Because Lua is an easy to learn
				language, reading a few first chapters of
				[the official book][urls.lua_pil] or consulting
				[the manual][urls.lua_manual] would suffice.
				"""#
		}
		search_dirs: {
			title: "Search Directories"
			body: #"""
				Vector provides a `search_dirs` option that allows you to specify
				absolute paths that will be searched when using the
				[Lua `require` function][urls.lua_require]. If this option is not
				set, the directories of the
				[configuration files][docs.setup.configuration] will be used
				instead.
				"""#
		}
	}
}
