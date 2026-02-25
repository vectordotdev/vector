package metadata

configuration: {
	configuration: #Schema | {
		enrichment_tables: #SchemaField | {
			outputs: [components.#Output, ...components.#Output]
		}
	}
	how_it_works: #HowItWorks
}

configuration: {
	configuration: generated.configuration.configuration

	configuration: {
		// expire_metrics's type is a little bit tricky, we could not generate `uint` from `docs::type_override` metadata macro easily.
		// So we have to define it manually, which is okay because it is already deprecated and it will be deleted soon.
		expire_metrics: {
			common: false
			description: """
				If set, Vector will configure the internal metrics system to automatically
				remove all metrics that have not been updated in the given time.

				If set to a negative value expiration is disabled.
				"""
			required: false
			warnings: ["Deprecated, please use `expire_metrics_secs` instead."]
			type: object: options: {
				secs: {
					common:      true
					required:    false
					description: "The whole number of seconds after which to expire metrics."
					type: uint: {
						default: null
						examples: [60]
						unit: "seconds"
					}
				}
				nsecs: {
					common:      true
					required:    false
					description: "The fractional number of seconds after which to expire metrics."
					type: uint: {
						default: null
						examples: [0]
						unit: "nanoseconds"
					}
				}
			}
		}

		enrichment_tables: {
			outputs: [
				{
					name: components._default_output.name
					description: """
						Default output stream. Only applies to memory enrichment table. Only active if `source_config.export_interval` is defined. Use `<source_config.source_key>` as an input to downstream transforms and sinks.
						"""
				},
				{
					name: "expired"
					description: """
						Output stream of expired items. Only applies to memory enrichment table. Only active if `source_config.export_expired_items` is enabled. Use `<source_config.source_key>.expired` as an input to downstream transforms and sinks.
						"""
				},
			]
		}
	}
}
