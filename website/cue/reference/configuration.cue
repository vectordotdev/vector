package metadata

configuration: {
	configuration: #Schema
	how_it_works:  #HowItWorks
}

configuration: {
	configuration: base.configuration.configuration

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

		expire_metrics_per_metric_set: {
			type: array: {
				items: type: object: options: {
					labels: {
						description: """
						Labels to apply this expiration to. Ignores labels if not defined.
						"""
						required: false
						type: object: options: {
							type: {
								required: true
								type: string: enum: {
									exact: "Looks for an exact match of one label key value pair."
									regex: "Compares label value with given key to the provided pattern."
									all:   "Checks that all of the provided matchers can be applied to given metric."
									any:   "Checks that any of the provided matchers can be applied to given metric."
								}
								description: "Metric label matcher type."
							}
							key: {
								required: true
								type: string: {}
								description:   "Metric key to look for."
								relevant_when: "type = \"exact\" or type = \"regex\""
							}
							value: {
								required: true
								type: string: {}
								description:   "The exact metric label value."
								relevant_when: "type = \"exact\""
							}
							pattern: {
								required: true
								type: string: {}
								description:   "Pattern to compare metric label value to."
								relevant_when: "type = \"regex\""
							}
							matchers: {
								required: true
								type: array: items: type: object: {}
								description: """
								List of matchers to check. Each matcher has the same
								options as the `labels` object.
								"""
								relevant_when: "type = \"all\" or type = \"any\""
							}
						}
					}
				}
			}
		}
	}
}
