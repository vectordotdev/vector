// Root
//
// The root file defines the schema for all of Vector's reference metadata.
// It does not include boilerplate or domain specific policies.

package metadata

#ConfigurationOptions: [Name=string]: {
	// `desription` describes the option in a succient fashion. Usually 1 to
	// 2 sentences.
	description: string

	// `groups` groups options into categories.
	//
	// For example, the `influxdb_logs` sink supports both v1 and v2 of Influxdb
	// and relevant options are placed in those groups.
	groups?: [...string]

	// `name` sets the name for this option. It is automatically set for you
	// via the key you use.
	name:           Name

	// `relevant_when` clarifies when an option is relevant.
	//
	// For example, if an option depends on the value of another option you can
	// specify that here. We accept a string to allow for the expression of
	// complex requirements.
	//
	// 		relevant_when: '`strategy` = "fingerprint"'
	//		relevant_when: '`strategy` = "fingerprint" or "inode"'
	relevant_when?: string

	// `required` requires the option to be set.
	required:       bool

	// `warnings` warn the user about aspect of the option.
	//
	// For example, the `tls.verify_hostname` option has a warning about
	// reduced security if the option is disabled.
	warnings: [...{
		visibility_level: "component" | "option"
		text:             string
	}]

	if !required {
		// `common` specifes that the option is commonly used. It will bring the
		// option to the top of the documents, surfacing it from other
		// non-common options.
		common: bool
	}

	// `sort` sorts the option, otherwise options will be sorted alphabetically.
	sort?: int8

	// `types` sets the option's value type. External tagging is used since
	// each type has it's own set of fields.
	type: {
		// `*` represents a wildcard type.
		//
		// For example, the `sinks.http.headers.*` option allows for arbitrary
		// key/value pairs.
		"*"?: {}

		// `[string]` represents an array of strings type.
		"[string]"?: {
			if !required {
				// `default` sets the default value.
				default: [...string] | null
			}

			// `enum` restricts the value to a set of values.
			//
			//		enum: {
			//			json: "Encodes the data via application/json"
			//			text: "Encodes the data via text/plain"
			//		}
			enum?: [Name=_]: string


			// `examples` clarify values through examples. This should be used
			// when examples cannot be derived from the `default` or `enum`
			// options.
			examples: [...[...string]] | *[[
					for k, v in enum {
					k
				},
			]]

			// `templateable` means that the option supports dynamic templated
			// values.
			templateable?: bool
		}

		// `bool` represents a boolean tool.
		"bool"?: {
			if !required {
				// `default` sets the default value.
				default: bool | null
			}
		}

		// `object` represents an object type that contains child options.
		"object"?: {
			// `examples` clarify values through examples. This should be used
			// when examples cannot be derived from the `default` or `enum`
			// options.
			examples: [...{[Name=string]: _}]

			// `options` represent the child options for this option.
			options: #ConfigurationOptions | {}
		}

		// `strings` represents a string type.
		"string"?: {
			if !required {
				// `default` sets the default value.
				default: string | null
			}

			// `enum` restricts the value to a set of values.
			//
			//		enum: {
			//			json: "Encodes the data via application/json"
			//			text: "Encodes the data via text/plain"
			//		}
			enum?: [Name=_]: string

			// `examples` demonstrates example values. This should be used when
			// examples cannot be derived from the `default` or `enum` options.
			examples: [...string] | *[
					for k, v in enum {
					k
				},
			]

			// `templateable` means that the option supports dynamic templated
			// values.
			templateable?: bool
		}

		// `uint` represents a positive integer type.
		"uint"?: {
			if !required {
				// `default` sets the default value.
				default: uint | null
			}

			// `examples` clarify values through examples. This should be used
			// when examples cannot be derived from the `default` or `enum`
			// options.
			examples?: [...uint]

			// `unit` clarifies the value's unit. While this should be included
			// as the suffix in the name, this helps to explicitly clarify that.
			unit: "bytes" | "logs" | "milliseconds" | "seconds" | null
		}
	}
}

#Components: [Type=string]: {
	// `kind` specified the component kind. This is set automatically.
	kind: "sink" | "source" | "transform"

	// `long_description` describes the components with a single paragraph.
	// It is used for SEO purposes and should be full of relevant keywords.
	long_description: string

	// `short_description` describes the component in one sentence.
	short_description: string

	// `title` is the human friendly title for the component.
	//
	// For example, the `http` sink has a `HTTP` title.
	title: string

	// `type` is the component identifier. This is set automatically.
	type: Type

	// `classes` represent the various classifications for this component
	classes: {
		// `commonly_used` specifies if the component is commonly used or not.
		// Setting this to `true` will surface the component from othere
		// non commonly used components.
		commonly_used: bool

		if kind == "source" {
			// `deployment_roles` clarify when a component should be used under
			// certain deployment contexts.
			//
			// * `daemon` - Vector is installed as a single process on the host.
			// * `sidecar` - Vector is installed along side each process it is
			//   monitoring. Therefore, there might be multiple Vector processes
			//   on the host.
			// * `service` - Vector receives data from one or more upstream
			//   sources, typically over a network protocol.
			deployment_roles: ["daemon" | "service" | "sidecar", ...]
		}

		// `egress_method` documents how the component outputs events.
		//
		// * `batch` - one or more events at a time
		// * `stream` - one event at a time
		egress_method: "batch" | "stream"

		// `function` specified the functions behavior categories. This helps
		// with component filtering. Each component type will allow different
		// functions.
		function: string

		if kind == "sink" {
			// `service_providers` specify the service providers that support
			// and host this service. This helps users find relevant sinks.
			//
			// For example, "AWS" is a service provider for many services, and
			// a user on AWS can use this to filter for AWS supported
			// components.
			service_providers: [...string]
		}
	}

	// `features` describes the various supported features of the component.
	// Setting these helps to reduce boilerplate.
	//
	// For example, the `tls` feature will automatically add the appropriate
	// `tls` options and `how_it_works` sections.
	features: close({
		if kind == "sink" && classes.egress_method == "batch" {
			// `batch` describes how the component batches data. This is only
			// relevant if a component has an `egress_method` of "batch".
			batch: close({
				enabled:      bool
				common:       bool
				max_bytes:    uint | null
				max_events:   uint | null
				timeout_secs: uint8
			})
		}

		if kind == "sink" {
			// `buffer` describes how the component buffers data.
			buffer: close({
				enabled: bool | string
			})
		}

		if kind == "source" {
			// `checkpoint` describes how the component checkpoints it's read
			// position.
			checkpoint: close({
				enabled: bool
			})
		}

		if kind == "sink" {
			// `compression` describes how the component compresses data.
			compression: {
				enabled: bool

				if enabled == true {
					default: "gzip" | null
					gzip:    bool
				}
			}
		}

		if kind == "sink" {
			// `encoding` describes how the component encodes data.
			encoding: close({
				enabled: true

				if enabled {
					default: null
					json:    null
					ndjson:  null
					text:    null
				}
			})
		}

		if kind == "sink" {
			// `healtcheck` notes if a component offers a healthcheck on boot.
			healthcheck: close({
				enabled: bool
			})
		}

		if kind == "source" {
			// `multiline` should be enabled for sources that offer the ability
			// to merge multiple lines together.
			multiline: close({
				enabled: bool
			})
		}

		if kind == "sink" {
			// `request` describes how the component issues and manages external
			// requests.
			request: {
				enabled: bool

				if enabled {
					in_flight_limit:            uint8
					rate_limit_duration_secs:   uint8
					rate_limit_num:             uint8
					retry_initial_backoff_secs: uint8
					retry_max_duration_secs:    uint8
					timeout_secs:               uint8
				}
			}
		}

		if kind == "source" || kind == "sink" {
			// `tls` describes if the component secures network communication
			// via TLS.
			tls: {
				enabled: bool

				if enabled {
					can_enable:             bool
					can_verify_certificate: bool
					if kind == "sink" {
						can_verify_hostname: bool
					}
					enabled_default: bool
				}
			}
		}
	})

	// `statuses` communicates the various statuses of the component.
	statuses: {
		if kind == "source" || kind == "sink" {
			// The delivery status. At least once means we guarantee that events
			// will be delivered at least once. Best effort means there is potential
			// for data loss.
			delivery: "at_least_once" | "best_effort"
		}

		// The developmnet status of this component. Beta means the component is
		// new and has not proven to be stable. Prod ready means that component
		// is reliable and settled.
		development: "beta" | "stable" | "deprecated"
	}

	// `support` communicates the varying levels of support of the component.
	support: {
		if kind == "transform" || kind == "sink" {
			input_types: ["log" | "metric", ...]
		}

		// `platforms` describes which platforms this component is available on.
		//
		// For example, the `journald` source is only available on Linux
		// environments.
		platforms: {
			"aarch64-unknown-linux-gnu":  bool
			"aarch64-unknown-linux-musl": bool
			"x86_64-apple-darwin":        bool
			"x86_64-pc-windows-msv":      bool
			"x86_64-unknown-linux-gnu":   bool
			"x86_64-unknown-linux-musl":  bool
		}

		// `requirements` describes any external requirements that the component
		// needs to function properly.
		//
		// For example, the `journald` source requires the presence of the
		// `journalctl` binary.
		requirements: [...string] | null

		// `warnings` describes any warning the user should know about the
		// component.
		//
		// For example, the `grok_parser` might offer a performance warning
		// since the `regex_parser` and other transforms are faster.
		warnings: [...string] | null

		// `notices` communicate useful information to the user that is neither
		// a requirement or warning.
		//
		// For example, the `lua` transform offers a Lua version notice that
		// communicate which version of Lua is embedded.
		notices: [...string] | null
	}

	configuration: #ConfigurationOptions

	if kind == "source" || kind == "transform" {
		// `output` documents output of the component. This is very important
		// as it communicate which events and fields are emitted.
		output: {
			logs?:    #LogOutput
			metrics?: #MetricOutput
		}
	}

	// `examples` demonstrates various ways to use the component using an
	// input, output, and example configuration.
	examples: {
		log: [
			...{
				title: string
				"configuration": {
					for k, v in configuration {
						"\( k )"?: _ | *null
					}
				}
				input: #LogEvent | [#LogEvent, ...] | string

				if classes.egress_method == "batch" {
					output: [#LogEvent, ...] | null
				}

				if classes.egress_method == "stream" {
					output: #LogEvent | null
				}

				notes?: string
			},
		]
		metric: [
			...{
				title: string
				"configuration": {
					for k, v in configuration {
						"\( k )"?: _ | *null
					}
				}
				input: #MetricEvent

				if classes.egress_method == "batch" {
					output: [#MetricEvent, ...] | null
				}

				if classes.egress_method == "stream" {
					output: #MetricEvent | null
				}

				notes?: string
			},
		]
	}

	// `how_it_works` contain sections that further describe the component's
	// behavior. This is like a mini-manual for the component and should help
	// answer any obvious questions the user might have. Options can links
	// to these sections for deeper explanations of behavior.
	how_it_works: #HowItWorks
}

#LogEvent: [Name=string]: #LogEvent | _

#MetricEvent: {
	counter: {
		value: uint
	}
	tags: [Name=string]: string
}

#HowItWorks: [Name=string]: {
	name:  Name
	title: string
	body:  string
	sub_sections?: [...{
		title: string
		body:  string
	}]
}

#LogOutput: [Name=string]: {
	description: string
	name:        Name
	fields: [Name=string]: {
		description:    string
		name:           Name
		relevant_when?: string
		required:       bool
		type: {
			"*": {}
			"[string]"?: {
				examples: [[string, ...string], ...[string, ...string]]
			}
			"string"?: {
				examples: [string, ...string]
			}
			"timestamp"?: {
				examples: ["2020-11-01T21:15:47.443232Z"]
			}
		}
	}
}

#MetricOutput: [Name=string]: {
	description:    string
	relevant_when?: string
	tags: [Name=string]: {
		description: string
		examples: [string, ...]
		required: bool
		name:     Name
	}
	name: Name
	type: "counter" | "gauge" | "histogram" | "summary"
}

components: close({
	sources:    #Components
	transforms: #Components
	sinks:      #Components
})
