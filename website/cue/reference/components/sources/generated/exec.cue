package metadata

generated: components: sources: exec: configuration: {
	clear_environment: {
		description: "Whether or not to clear the environment before setting custom environment variables."
		required:    false
		type: bool: default: false
	}
	command: {
		description: "The command to run, plus any arguments required."
		required:    true
		type: array: items: type: string: examples: ["echo", "Hello World!"]
	}
	environment: {
		description: """
			Custom environment variables to set or update when running the command.
			If a variable name already exists in the environment, its value is replaced.
			"""
		required: false
		type: object: {
			examples: [{
				LANG: "es_ES.UTF-8"
				PATH: "/bin:/usr/bin:/usr/local/bin"
				TZ:   "Etc/UTC"
			}]
			options: "*": {
				description: "An environment variable."
				required:    true
				type: string: {}
			}
		}
	}
	include_stderr: {
		description: "Whether or not the output from stderr should be included when generating events."
		required:    false
		type: bool: default: true
	}
	maximum_buffer_size_bytes: {
		description: "The maximum buffer size allowed before a log event is generated."
		required:    false
		type: uint: default: 1000000
	}
	mode: {
		description: "Mode of operation for running the command."
		required:    true
		type: string: enum: {
			scheduled: "The command is run on a schedule."
			streaming: "The command is run until it exits, potentially being restarted."
		}
	}
	scheduled: {
		description: "Configuration options for scheduled commands."
		required:    false
		type: object: options: exec_interval_secs: {
			description: """
				The interval, in seconds, between scheduled command runs.

				If the command takes longer than `exec_interval_secs` to run, it is killed.
				"""
			required: false
			type: uint: default: 60
		}
	}
	streaming: {
		description: "Configuration options for streaming commands."
		required:    false
		type: object: options: {
			respawn_interval_secs: {
				description: "The amount of time, in seconds, before rerunning a streaming command that exited."
				required:    false
				type: uint: default: 5
			}
			respawn_on_exit: {
				description: "Whether or not the command should be rerun if the command exits."
				required:    false
				type: bool: default: true
			}
		}
	}
	working_directory: {
		description: "The directory in which to run the command."
		required:    false
		type: string: {}
	}
}

generated: components: sources: exec: configuration: decoding: decodingBase & {
	type: object: options: codec: {
		required: false
		type: string: default: "bytes"
	}
}
generated: components: sources: exec: configuration: framing: framingDecoderBase & {
	type: object: options: method: required: true
}
