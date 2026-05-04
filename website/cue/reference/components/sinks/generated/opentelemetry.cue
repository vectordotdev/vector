package metadata

generated: components: sinks: opentelemetry: configuration: {}

generated: components: sinks: opentelemetry: configuration: protocol: framingEncoderBase & {
	type: object: options: method: {
		required: false
		type: string: default: "post"
	}
}
