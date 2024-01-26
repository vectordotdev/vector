package metadata

components: sinks: _datadog: {
	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   string | *"stable"
		egress_method: "batch"
		service_providers: ["Datadog"]
		stateful: bool | *false
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		default_api_key: {
			description: "Default Datadog [API key](https://docs.datadoghq.com/api/?lang=bash#authentication), if an event has a key set in its metadata it will prevail over the one set here."
			required:    true
			warnings: []
			type: string: {
				examples: ["${DATADOG_API_KEY_ENV_VAR}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		endpoint: {
			common:        false
			description:   "The endpoint to send data to."
			relevant_when: "site is not set"
			required:      false
			type: string: {
				default: null
				examples: ["http://127.0.0.1:8080", "http://example.com:12345"]
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
