package metadata

// These sources produce JSON providing a structured representation of the
// Vector CLI (commands, flags, etc.)

#Args: [Arg=string]: {
	description: !=""
	type:        #ArgType
	default?:    string | [...string]
	required:    bool | *false

	if default == _|_ {
		required: true
	}
}

#ArgType: "string" | "list"

#CommandLineTool: {
	name:     !=""
	flags:    #Flags
	options:  #Options
	commands: #Commands
}

#Commands: [Command=string]: {
	description: !=""
	flags?:      #Flags
	options?:    #Options
	args?:       #Args
}

#Flags: [Flag=string]: {
	flag:        "--\(Flag)"
	description: !=""
	env_var?:    string

	if _short != _|_ {
		short: "-\(_short)"
	}

	_short: !=""
}

#Options: [Option=string]: {
	option:      "--\(Option)"
	description: !=""
	default?:    string | int
	enum?: [...string]
	type:     #OptionType
	env_var?: string
	example?: string
	required: bool | *false

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

_default_flags: {
	"help": {
		_short:      "h"
		description: "Prints help information"
	}
	"version": {
		_short:      "V"
		description: "Prints version information"
	}
}

cli: #CommandLineTool & {
	name: "vector"

	flags: _default_flags & {
		"quiet": {
			_short: "q"
			description: """
				Reduce detail of internal logging. Repeat to reduce further. Overrides
				`--verbose`
				"""
		}
		"require-healthy": {
			_short:      "r"
			description: "Exit on startup if any sinks fail healthchecks"
			env_var:     "VECTOR_REQUIRE_HEALTHY"
		}
		"verbose": {
			_short:      "v"
			description: "Enable more detailed logging. Repeat to reduce further. Overrides `--verbose`"
		}
		"watch-config": {
			_short:      "w"
			description: "Watch for changes in the configuration file, and reload accordingly"
			env_var:     "VECTOR_WATCH_CONFIG"
		}
	}

	options: {
		"color": {
			description: """
				Control when ANSI terminal formatting is used.

				By default `vector` will try and detect if `stdout` is a terminal,
				if it is ANSI will be enabled. Otherwise it will be disabled. By
				providing this flag with the `--color always` option will always
				enable ANSI terminal formatting. `--color never` will disable all
				ANSI terminal formatting. `--color auto` will attempt to detect it
				automatically.
				"""
			default: "auto"
			enum: ["always", "auto", "never"]
		}
		"config": {
			_short: "c"
			description: """
				Read configuration from one or more files. Wildcard paths are
				supported. If zero files are specified the default config path
				`/etc/vector/vector.toml` will be targeted
				"""
			type:    "string"
			default: "/etc/vector/vector.toml"
			env_var: "VECTOR_CONFIG"
		}
		"threads": {
			_short: "t"
			description: """
				Number of threads to use for processing (default is number of
				available cores)
				"""
			type:    "integer"
			env_var: "VECTOR_THREADS"
		}
		"log-format": {
			description: "Set the logging format [default: text]"
			default:     "text"
			enum: ["json", "text"]
		}
	}

	commands: {
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
				expression: {
					description: """
						Generate expression, e.g. `stdin/json_parser,add_fields/console`

						Three comma-separated lists of sources, transforms and sinks, divided
						by forward slashes. If subsequent component types are not needed then
						their dividers can be omitted from the expression.

						For example:

						`/json_parser` prints a `json_parser` transform.

						`//file,http` prints a `file` and `http` sink.

						`stdin//http` prints a `stdin` source an an `http` sink.

						Generated components are given incremental names (`source1`,
						`source2`, etc) which should be replaced in order to provide better
						context. You can optionally specify the names of components by
						prefixing them with `<name>:`, e.g.:

						`foo:stdin/bar:regex_parser/baz:http` prints a `stdin` source called
						`foo`, a `regex_parser` transform called `bar`, and an `http` sink
						called `baz`.

						Vector makes a best attempt at constructing a sensible topology. The
						first transform generated will consume from all sources and subsequent
						transforms will consume from their predecessor. All sinks will consume
						from the last transform or, if none are specified, from all sources.
						It is then up to you to restructure the `inputs` of each component to
						build the topology you need.
						"""
					type: "string"
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
					enum: ["json", "text"]
				}
			}
		}

		"test": {
			description: """
				Run Vector config unit tests, then exit. This command is experimental and
				therefore subject to change. For guidance on how to write unit tests check
				out: https://vector.dev/docs/setup/guides/unit-testing/
				"""

			args: {
				paths: _paths_arg & {
					description: """
						Any number of Vector config files to test. If none are specified
						the default config path `/etc/vector/vector.toml` will be targeted
						"""
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

			args: {
				paths: _paths_arg & {
					description: """
						Any number of Vector config files to validate. If none are specified
						the default config path `/etc/vector/vector.toml` will be targeted
						"""
				}
			}
		}
	}

	// Helpers
	_paths_arg: {
		type:    "list"
		default: "/etc/vector/vector.toml"
	}
}
