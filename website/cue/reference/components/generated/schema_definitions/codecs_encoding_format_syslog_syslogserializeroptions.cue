package metadata

_schemaDefinitions: "codecs::encoding::format::syslog::SyslogSerializerOptions": object: options: {
	app_name: {
		description: """
			Path to a field in the event to use for the app name.

			If not provided, the encoder checks for a semantic "service" field.
			If that is also missing, it defaults to "vector".
			"""
		required: false
		type: string: {}
	}
	facility: {
		description: "Path to a field in the event to use for the facility. Defaults to \"user\"."
		required:    false
		type: string: {}
	}
	msg_id: {
		description: "Path to a field in the event to use for the msg ID."
		required:    false
		type: string: {}
	}
	proc_id: {
		description: "Path to a field in the event to use for the proc ID."
		required:    false
		type: string: {}
	}
	rfc: {
		description: "RFC to use for formatting."
		required:    false
		type: string: {
			default: "rfc5424"
			enum: {
				rfc3164: "The legacy RFC3164 syslog format."
				rfc5424: "The modern RFC5424 syslog format."
			}
		}
	}
	severity: {
		description: "Path to a field in the event to use for the severity. Defaults to \"informational\"."
		required:    false
		type: string: {}
	}
}
