package metadata

components: sinks: _datadog: {
	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   string | *"stable"
		egress_method: "batch"
		service_providers: ["Datadog"]
		stateful: false
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		api_key: {
			description: "Datadog [API key](https://docs.datadoghq.com/api/?lang=bash#authentication)"
			required:    true
			type: string: {
				examples: ["${DATADOG_API_KEY_ENV_VAR}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		region: {
			common:        false
			description:   "The region to send data to."
			required:      false
			relevant_when: "endpoint is not set"
			warnings: ["This option has been deprecated, the `site` option should be used."]
			type: string: {
				default: null
				enum: {
					us: "United States"
					eu: "Europe"
				}
			}
		}
		site: {
			common:        false
			description:   "The [Datadog site](https://docs.datadoghq.com/getting_started/site) to send data to. "
			required:      false
			relevant_when: "endpoint is not set"
			type: string: {
				default: "datadoghq.com"
				examples: ["us3.datadoghq.com", "datadoghq.com", "datadoghq.eu"]
			}
		}
	}
}
