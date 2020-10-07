// Root
//
// The root file defines the schema for all of Vector's reference metadata.
// It does not include boilerplate or domain specific policies.

package metadata

#ConfigurationOptions: [Name=string]: {
	description: string
	groups?: [...string]
	name:           Name
	relevant_when?: string
	required:       bool

	warnings: [...{
		visibility_level: "component" | "option"
		text:             string
	}]

	if !required {
		common: bool
	}

	sort?: int8

	type: {
		"[string]"?: {
			if !required {
				default: [...string] | null
			}

			enum?: [Name=_]: string

			examples: [...[...string]] | *[[
					for k, v in enum {
					k
				},
			]]

			templateable?: bool
		}
		"bool"?: {
			if !required {
				default: bool | null
			}
		}
		"object"?: {
			examples: [...{[Name=string]: _}]
			options: #ConfigurationOptions | {}
		}
		"string"?: {
			if !required {
				default: string | null
			}

			enum?: [Name=_]: string

			examples: [...string] | *[
					for k, v in enum {
					k
				},
			]

			templateable?: bool
		}
		"uint"?: {
			if !required {
				default: uint | null
			}
			examples?: [...uint]
			unit: "bytes" | "logs" | "milliseconds" | "seconds" | null
		}
	}
}

#Components: [Type=string]: {
	// The component kind. This is set automatically.
	kind: "sink" | "source" | "transform"

	// A long description of the component, full of relevant keywords for SEO
	// purposes.
	long_description: string

	// A short, one sentence description.
	short_description: string

	// The component title, used in text. For example, the `http` source has
	// a title of "HTTP".
	title: string

	// The component type. This is set automatically.
	type: Type

	// Classes represent the various classifications for this component
	classes: {
		// Is this component commonly used? If so, we'll feature it in various
		// sections in our documentation.
		commonly_used: bool

		if kind == "source" {
			// The deploment roles that this source is applicable in.
			// For example, you would not use the `file` source in the `service`
			// role.
			deployment_roles: ["daemon" | "service" | "sidecar", ...]
		}

		// The behavior function for this component. This is used as a filter to
		// help users find components that serve a function.
		function: string

		if kind == "sink" {
			// Any service providers that host the downstream service.
			service_providers: [...string]
		}
	}

	features: close({
		if kind == "sink" {
			batch: close({
				enabled:      bool
				common:       bool
				max_bytes:    uint | null
				max_events:   uint | null
				timeout_secs: uint8
			})
		}

		if kind == "sink" {
			buffer: close({
				enabled: bool | string
			})
		}

		if kind == "source" {
			checkpoint: close({
				enabled: bool
			})
		}

		if kind == "sink" {
			compression: {
				enabled: bool

				if enabled == true {
					default: "gzip" | null
					gzip:    bool
				}
			}
		}

		if kind == "sink" {
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
			healthcheck: close({
				enabled: bool
			})
		}

		if kind == "source" {
			multiline: close({
				enabled: bool
			})
		}

		if kind == "sink" {
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

	// The various statuses of this component.
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

	// Various support details for the component.
	support: {
		if kind == "transform" || kind == "sink" {
			input_types: ["log" | "metric", ...]
		}

		// The platforms that this component is available in. It is possible for
		// Vector to disable some components on a per-platform basis.
		platforms: {
			"aarch64-unknown-linux-gnu":  bool
			"aarch64-unknown-linux-musl": bool
			"x86_64-apple-darwin":        bool
			"x86_64-pc-windows-msv":      bool
			"x86_64-unknown-linux-gnu":   bool
			"x86_64-unknown-linux-musl":  bool
		}

		// Any requirements for this component to work properly. This should note
		// external dependencies or configuration. These will be displayed
		// prominently at the top of the component's docs.
		requirements: [...string] | null

		// Any warnings for this component. This should address any "gotchas" as
		// part of using this source.
		warnings: [...string] | null
	}

	configuration: #ConfigurationOptions

	// Output events for the component.
	if kind == "source" || kind == "transform" {
		output: {
			logs?:    #LogEvents
			metrics?: #MetricEvents
		}
	}

	// Example uses for the component.
	examples: {
		log: [
			...{
				title: string
				"configuration": {
					for k, v in configuration {
						"\( k )"?: _ | *null
					}
				}
				input:    #Fields | [#Fields, ...] | string
				"output": #Fields
			},
		]
	}

	// Markdown-based sections that describe how the component works.
	how_it_works: #HowItWorks
}

#Fields: [Name=string]: #Fields | _

#HowItWorks: [Name=string]: {
	name:  Name
	title: string
	body:  string
	sub_sections?: [...{
		title: string
		body:  string
	}]
}

#LogEvents: [Name=string]: {
	description: string
	name:        Name
	fields: [Name=string]: {
		description:    string
		name:           Name
		relevant_when?: string
		required:       bool
		type: {
			"*": {}
			"string"?: {
				examples: [string, ...string]
			}
			"timestamp"?: {
				examples: ["2020-11-01T21:15:47.443232Z"]
			}
		}
	}
}

#MetricEvents: [Name=string]: {
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
