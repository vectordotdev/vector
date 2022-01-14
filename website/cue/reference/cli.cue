package metadata

// These sources produce JSON providing a structured representation of the
// Vector CLI (commands, flags, etc.)
_default_flags: {
	"help": {
		_short:      "h"
		description: "Prints help information "
	}
	"version": {
		_short:      "V"
		description: "Prints version information"
	}
}

cli: {
	#Args: [Arg=string]: {
		description: !=""
		required:    bool | *false
		name:        Arg
		type:        #ArgType
		default?:    string | [...string]
	}

	#ArgType: "string" | "list"

	#Commands: [Command=string]: {
		description:  !=""
		name:         Command
		example?:     string
		flags?:       #Flags
		options?:     #Options
		args?:        #Args
		experimental: bool | *false
	}

	#Flags: [Flag=string]: {
		flag:         "--\(Flag)"
		description:  string
		env_var?:     string
		name:         Flag
		experimental: bool | *false

		if _short != _|_ {
			short: "-\(_short)"
		}

		_short: string
	}

	#Options: [Option=string]: {
		option:       "--\(Option)"
		default?:     string | int
		description:  string
		enum?:        #Enum
		name:         Option
		type:         #OptionType
		env_var?:     string
		example?:     string
		required:     bool | *false
		experimental: bool | *false

		if default == _|_ {
			required: true
		}

		if _short != _|_ {
			short: "-\(_short)"
		}

		_short: !=""

		if enum != _|_ {
			type: "enum"
		}
	}

	#OptionType: "string" | "integer" | "enum"

	name:     !=""
	flags:    #Flags
	options:  #Options
	commands: #Commands

	env_vars: #EnvVars
}

cli: {
	name: "vector"

	flags: _default_flags & {
		"quiet": {
			_short: "q"
			description: """
				Reduce detail of internal logging. Repeat to reduce further. Overrides `--verbose`.
				"""
		}
		"require-healthy": {
			_short:      "r"
			description: env_vars.VECTOR_REQUIRE_HEALTHY.description
			env_var:     "VECTOR_REQUIRE_HEALTHY"
		}
		"verbose": {
			_short:      "v"
			description: "Enable more detailed logging. Repeat to reduce further. Overrides `--verbose`."
		}
		"watch-config": {
			_short:      "w"
			description: env_vars.VECTOR_WATCH_CONFIG.description
			env_var:     "VECTOR_WATCH_CONFIG"
		}
	}

	// Reusable options
	_core_options: {
		"color": {
			description: env_vars.VECTOR_COLOR.description
			default:     env_vars.VECTOR_COLOR.type.string.default
			enum:        env_vars.VECTOR_COLOR.type.string.enum
			env_var:     "VECTOR_COLOR"
		}
		"config": {
			_short:      "c"
			description: env_vars.VECTOR_CONFIG.description
			type:        "string"
			default:     env_vars.VECTOR_CONFIG.type.string.default
			env_var:     "VECTOR_CONFIG"
		}
		"config-dir": {
			description: env_vars.VECTOR_CONFIG_DIR.description
			type:        "string"
			env_var:     "VECTOR_CONFIG_DIR"
		}
		"config-toml": {
			description: env_vars.VECTOR_CONFIG_TOML.description
			type:        "string"
			env_var:     "VECTOR_CONFIG_TOML"
		}
		"config-json": {
			description: env_vars.VECTOR_CONFIG_JSON.description
			type:        "string"
			env_var:     "VECTOR_CONFIG_JSON"
		}
		"config-yaml": {
			description: env_vars.VECTOR_CONFIG_YAML.description
			type:        "string"
			env_var:     "VECTOR_CONFIG_YAML"
		}
		"log-format": {
			description: env_vars.VECTOR_LOG_FORMAT.description
			default:     env_vars.VECTOR_LOG_FORMAT.type.string.default
			enum:        env_vars.VECTOR_LOG_FORMAT.type.string.enum
			env_var:     "VECTOR_LOG_FORMAT"
		}

		"threads": {
			_short:      "t"
			description: env_vars.VECTOR_THREADS.description
			type:        "integer"
			env_var:     "VECTOR_THREADS"
		}
	}

	options: _core_options

	commands: {
		"graph": {
			description: """
				Generate a visual representation of topologies. The output is in the [DOT format](\(urls.dot_format)),
				which can be rendered using [GraphViz](\(urls.graphviz)).

				You can also visualize the output online at [webgraphviz.com](http://www.webgraphviz.com/).
				"""

			example: "vector graph --config /etc/vector/vector.toml | dot -Tsvg > graph.svg"

			options: _core_options
		}
		"generate": {
			description: "Generate a Vector configuration containing a list of components"

			flags: _default_flags & {
				"fragment": {
					_short:      "f"
					description: "Whether to skip the generation of global fields"
				}
			}

			options: {
				"file": {
					description: "Generate config as a file"
					type:        "string"
					example:     "/etc/vector/my-config.toml"
				}
			}

			args: {
				pipeline: {
					description: "Pipeline expression, e.g. `stdin/json_parser,add_fields/console`"
					type:        "string"
				}
			}
		}

		"help": {
			description: "Prints this message or the help of the given subcommand(s)"
		}

		"list": {
			description: "List available components, then exit"

			flags: _default_flags

			options: {
				"format": {
					description: "Format the list in an encoding schema"
					default:     "text"
					enum: {
						avro: "Output components in Apache Avro format"
						json: "Output components as JSON"
						text: "Output components as text"
					}
				}
			}
		}

		"test": {
			description: """
				Run Vector config unit tests, then exit. This command is experimental and
				therefore subject to change. For guidance on how to write unit tests check
				out the [unit testing documentation](\(urls.vector_unit_tests)).
				"""

			options: {
				"config-toml": {
					description: env_vars.VECTOR_CONFIG_TOML.description
					type:        "string"
					env_var:     "VECTOR_CONFIG_TOML"
				}
				"config-json": {
					description: env_vars.VECTOR_CONFIG_JSON.description
					type:        "string"
					env_var:     "VECTOR_CONFIG_JSON"
				}
				"config-yaml": {
					description: env_vars.VECTOR_CONFIG_YAML.description
					type:        "string"
					env_var:     "VECTOR_CONFIG_YAML"
				}
			}

			args: {
				paths: _paths_arg & {
					description: """
						Any number of Vector config files to test. If none are specified
						the default config path `/etc/vector/vector.toml` will be targeted
						"""
				}
			}
		}

		"tap": {
			description: """
				Observe output log events from source or transform components. Logs are sampled
				at a specified interval.
				"""

			flags: _default_flags

			options: {
				"interval": {
					_short:      "i"
					description: "Interval to sample logs at, in milliseconds"
					type:        "integer"
					default:     500
				}
				"url": {
					_short:      "u"
					description: "Vector GraphQL API server endpoint"
					type:        "string"
				}
				"limit": {
					_short:      "l"
					description: "Maximum number of log events to sample each interval"
					type:        "integer"
					default:     100
				}
				"format": {
					_short:      "f"
					description: "Encoding format for logs printed to screen"
					type:        "enum"
					default:     "json"
					enum: {
						json: "Output events as JSON"
						yaml: "Output events as YAML"
					}
				}
			}

			args: {
				components: {
					type: "list"
					description: """
						Components to observe (comma-separated; accepts glob patterns).
						"""
					default: "*"
				}
			}
		}

		"top": {
			description: """
				Display topology and metrics in the console, for a local or remote Vector
				instance
				"""

			flags: _default_flags & {
				"human-metrics": {
					_short: "h"
					description: """
						Humanize metrics, using numeric suffixes - e.g. 1,100 = 1.10 k,
						1,000,000 = 1.00 M
						"""
				}
			}

			options: {
				"refresh-interval": {
					_short:      "i"
					description: "How often the screen refreshes (in milliseconds)"
					type:        "integer"
					default:     500
				}
				"url": {
					_short:      "u"
					description: "The URL for the GraphQL endpoint of the running Vector instance"
					type:        "string"
				}
			}
		}

		"validate": {
			description: "Validate the target config, then exit"

			flags: _default_flags & {
				"no-environment": {
					_short: "ne"
					description: """
						Disables environment checks. That includes component
						checks and health checks
						"""
				}
				"deny-warnings": {
					_short:      "d"
					description: "Fail validation on warnings"
				}
			}

			options: {
				"config-toml": {
					description: """
						Any number of Vector config files to validate.
						TOML file format is assumed.
						"""
					type: "string"
				}
				"config-json": {
					description: """
						Any number of Vector config files to validate.
						JSON file format is assumed.
						"""
					type: "string"
				}
				"config-yaml": {
					description: """
						Any number of Vector config files to validate.
						YAML file format is assumed.
						"""
					type: "string"
				}
			}

			args: {
				paths: _paths_arg & {
					description: """
						Any number of Vector config files to validate. If none are specified
						the default config path `/etc/vector/vector.toml` will be targeted
						"""
				}
			}
		}

		"vrl": {
			description: "Vector Remap Language CLI"

			flags: _default_flags & {
				"print-object": {
					_short: "o"
					description: """
						Print the (modified) object, instead of the result of the final
						expression.

						The same result can be achieved by using `.` as the final expression.
						"""
				}
			}

			options: {
				"input": {
					_short: "i"
					description: """
						File containing the object(s) to manipulate. Leave empty to use stdin.
						"""
					type: "string"
				}

				"program": {
					_short: "p"
					description: """
						File containing the program to execute. Can be used instead of `PROGRAM`.
						"""
					type: "string"
				}
			}

			args: {
				program: {
					description: #"""
						The program to execute. For example, `".foo = true"` sets the object's `foo`
						field to `true`.
						"""#
					type: "string"
				}
			}
		}
	}

	env_vars: {
		PROCFS_ROOT: {
			description: """
				Sets an arbitrary path to the system's [procfs](\(urls.procfs)) root. This can be
				used to expose host metrics from within a container. Vector uses the system's
				`/proc` by default.
				"""
			type: string: default: null
		}
		RUST_BACKTRACE: {
			description: """
				Enables [Rust](\(urls.rust)) backtraces when errors are logged. We recommend using
				this only when debugging, as it can degrade Vector's performance.
				"""
			type: bool: default: false
		}
		SYSFS_ROOT: {
			description: """
				Sets an arbitrary path to the system's [sysfs](\(urls.sysfs)) root. This can be used
				to expose host metrics from within a container. Vector uses the system's `/sys` by
				default.
				"""
			type: string: {
				default: null
				examples: ["/mnt/host/sys"]
			}
		}
		VECTOR_COLOR: {
			description: "Control when ANSI terminal formatting is used."
			type: string: {
				default: "auto"
				enum: {
					always: "Always enable ANSI terminal formatting."
					auto:   "Detect ANSI terminal formatting and enable if supported."
					never:  "Disable ANSI terminal formatting."
				}
			}
		}
		VECTOR_CONFIG: {
			description: """
				Read configuration from one or more files. Wildcard paths are supported. If no files are
				specified the default config path `/etc/vector/vector.toml` is targeted. TOML, YAML and
				JSON file formats are supported. The format to interpret the file with is determined from
				the file extension (`.toml`, `.yaml`, `.json`). Vector falls back to TOML if it can't
				detect a supported format.
				"""
			type: string: {
				default: "/etc/vector/vector.toml"
			}
		}
		VECTOR_CONFIG_DIR: {
			description: """
				Read configuration from files in one or more directories. The file format is detected
				from the file name. Files not ending in `.toml`, `.json`, `.yaml`, or `.yml` are
				ignored.
				"""
			type: string: default: null
		}
		VECTOR_CONFIG_JSON: {
			description: """
				Read configuration from one or more files. Wildcard paths are supported. JSON file
				format is assumed.
				"""
			type: string: default: null
		}
		VECTOR_CONFIG_TOML: {
			description: """
				Test configuration from one or more files. Wildcard paths are
				supported. TOML file format is assumed.
				"""
			type: string: default: null
		}
		VECTOR_CONFIG_YAML: {
			description: """
				Read configuration from one or more files. Wildcard paths are supported. YAML file
				format is assumed.
				"""
			type: string: default: null
		}
		VECTOR_LOG: {
			description: "Vector's log level. Each log level includes messages from higher priority levels."
			type: string: {
				default: "INFO"
				enum: {
					ERROR: "Only show error logs. The same as `-qq`"
					WARN:  "Include warnings. The same as `-q`"
					INFO:  "Include logs about Vector's operation. This is the default."
					DEBUG: "Includes logs useful for debugging or troubleshooting Vector. The same as `-v`"
					TRACE: "Most verbose log level. Can be used for troubleshooting Vector. The same as `-vv`"
				}
				examples: ["DEBUG", "INFO"]
			}
		}
		VECTOR_LOG_FORMAT: {
			description: "Set the logging format"
			type: string: {
				default: "text"
				enum: {
					json: "Output Vector's logs as JSON."
					text: "Output Vector's logs as text."
				}
			}
		}
		VECTOR_REQUIRE_HEALTHY: {
			description: "Exit on startup if any sinks fail healthchecks."
			type: bool: default: false
		}
		VECTOR_THREADS: {
			description: """
				The number of threads to use for processing. The default is the number of available cores.
				"""
			type: uint: {
				default: null
				unit:    null
			}
		}
		VECTOR_WATCH_CONFIG: {
			description: "Watch for changes in the configuration file and reload accordingly"
			type: bool: default: false
		}
	}

	// Helpers
	_paths_arg: {
		type:    "list"
		default: "/etc/vector/vector.toml"
	}
}
