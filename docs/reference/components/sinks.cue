package metadata

components: sinks: [Name=string]: {
	kind: "sink"

	features: {
		send: {
			encoding: {
				codec: {
					enabled: bool

					if enabled {
						default: #EncodingCodec | null
						enum:    [#EncodingCodec, ...] | null
					}
				}
			}
		}
	}

	configuration: {
		encoding: {
			description: "Configures the encoding specific sink behavior."
			required:    true
			type: object: options: {
				if features.send.encoding.codec.enabled {
					codec: {
						description: "The encoding codec used to serialize the events before outputting."
						required:    true
						type: string: examples: features.send.encoding.codec.enum
					}
				}

				except_fields: {
					common:      false
					description: "Prevent the sink from encoding the specified labels."
					required:    false
					type: array: {
						default: null
						items: type: string: examples: ["message", "parent.child"]
					}
				}

				only_fields: {
					common:      false
					description: "Prevent the sink from encoding the specified labels."
					required:    false
					type: array: {
						default: null
						items: type: string: examples: ["message", "parent.child"]
					}
				}

				timestamp_format: {
					common:      false
					description: "How to format event timestamps."
					required:    false
					type: string: {
						default: "rfc3339"
						enum: {
							rfc3339: "Formats as a RFC3339 string"
							unix:    "Formats as a unix timestamp"
						}
					}
				}
			}
		}

		if sinks[Name].features.send != _|_ {
			if sinks[Name].features.send.tls.enabled {
				tls: configuration._tls & {_args: {
					can_enable:      sinks[Name].features.send.tls.can_enable
					enabled_default: sinks[Name].features.send.tls.enabled_default
				}}
			}
		}
	}
}
