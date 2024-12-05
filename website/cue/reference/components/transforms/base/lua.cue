package metadata

base: components: transforms: lua: configuration: {
	hooks: {
		description: """
			Lifecycle hooks.

			These hooks can be set to perform additional processing during the lifecycle of the transform.
			"""
		required: true
		type: object: options: {
			init: {
				description: """
					The function called when the first event comes in, before `hooks.process` is called.

					It can produce new events using the `emit` function.

					This can either be inline Lua that defines a closure to use, or the name of the Lua function to call. In both
					cases, the closure/function takes a single parameter, `emit`, which is a reference to a function for emitting events.
					"""
				required: false
				type: string: examples: ["""
					function (emit)
					\t-- Custom Lua code here
					end
					""", "init"]
			}
			process: {
				description: """
					The function called for each incoming event.

					It can produce new events using the `emit` function.

					This can either be inline Lua that defines a closure to use, or the name of the Lua function to call. In both
					cases, the closure/function takes two parameters. The first parameter, `event`, is the event being processed,
					while the second parameter, `emit`, is a reference to a function for emitting events.
					"""
				required: true
				type: string: examples: ["""
					function (event, emit)
					\tevent.log.field = "value" -- set value of a field
					\tevent.log.another_field = nil -- remove field
					\tevent.log.first, event.log.second = nil, event.log.first -- rename field
					\t-- Very important! Emit the processed event.
					\temit(event)
					end
					""", "process"]
			}
			shutdown: {
				description: """
					The function called when the transform is stopped.

					It can produce new events using the `emit` function.

					This can either be inline Lua that defines a closure to use, or the name of the Lua function to call. In both
					cases, the closure/function takes a single parameter, `emit`, which is a reference to a function for emitting events.
					"""
				required: false
				type: string: examples: ["""
					function (emit)
					\t-- Custom Lua code here
					end
					""", "shutdown"]
			}
		}
	}
	metric_tag_values: {
		description: """
			When set to `single`, metric tag values are exposed as single strings, the
			same as they were before this config option. Tags with multiple values show the last assigned value, and null values
			are ignored.

			When set to `full`, all metric tags are exposed as arrays of either string or null
			values.
			"""
		required: false
		type: string: {
			default: "single"
			enum: {
				full: "All tags are exposed as arrays of either string or null values."
				single: """
					Tag values are exposed as single strings, the same as they were before this config
					option. Tags with multiple values show the last assigned value, and null values
					are ignored.
					"""
			}
		}
	}
	search_dirs: {
		description: """
			A list of directories to search when loading a Lua file via the `require` function.

			If not specified, the modules are looked up in the configuration directories.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["/etc/vector/lua"]
		}
	}
	source: {
		description: """
			The Lua program to initialize the transform with.

			The program can be used to import external dependencies, as well as define the functions
			used for the various lifecycle hooks. However, it's not strictly required, as the lifecycle
			hooks can be configured directly with inline Lua source for each respective hook.
			"""
		required: false
		type: string: examples: ["""
			function init()
			\tcount = 0
			end

			function process()
			\tcount = count + 1
			end

			function timer_handler(emit)
			\temit(make_counter(counter))
			\tcounter = 0
			end

			function shutdown(emit)
			\temit(make_counter(counter))
			end

			function make_counter(value)
			\treturn metric = {
			\t\tname = "event_counter",
			\t\tkind = "incremental",
			\t\ttimestamp = os.date("!*t"),
			\t\tcounter = {
			\t\t\tvalue = value
			\t\t}
			 \t}
			end
			""", """
			-- external file with hooks and timers defined
			require('custom_module')
			"""]
	}
	timers: {
		description: "A list of timers which should be configured and executed periodically."
		required:    false
		type: array: {
			default: []
			items: type: object: options: {
				handler: {
					description: """
						The handler function which is called when the timer ticks.

						It can produce new events using the `emit` function.

						This can either be inline Lua that defines a closure to use, or the name of the Lua function
						to call. In both cases, the closure/function takes a single parameter, `emit`, which is a
						reference to a function for emitting events.
						"""
					required: true
					type: string: examples: ["timer_handler"]
				}
				interval_seconds: {
					description: "The interval to execute the handler, in seconds."
					required:    true
					type: uint: unit: "seconds"
				}
			}
		}
	}
	version: {
		description: """
			Transform API version.

			Specifying this version ensures that backward compatibility is not broken.
			"""
		required: true
		type: string: enum: {
			"1": """
				Lua transform API version 1.

				This version is deprecated and will be removed in a future version.
				"""
			"2": "Lua transform API version 2."
		}
	}
}
