package metadata

components: sinks: gcp_chronicle_unstructured: {
	title: "GCP Chronicle Unstructured"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["GCP"]
		stateful: false
	}

	features: {
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 300.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					framing: true
					enum: ["json", "text"]
				}
			}
			proxy: enabled: true
			request: {
				enabled:        true
				rate_limit_num: 1000
				headers:        false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.gcp_chronicle

				interface: {
					socket: {
						api: {
							title: "GCP XML Interface"
							url:   urls.gcp_xml_interface
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		api_key: configuration._gcp_api_key
		credentials_path: {
			category:    "Auth"
			common:      true
			description: "The filename for a Google Cloud service account credentials JSON file used to authenticate access to the Cloud Storage API. If this is unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename.\n\nIf no filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is running on. If Vector is not running on a GCE instance, you must define a credentials file as above."
			required:    false
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
			}
		}
		endpoint: {
			common:        false
			description:   "The endpoint to send data to."
			relevant_when: "region is not set"
			required:      false
			type: string: {
				default: null
				examples: ["127.0.0.1:8080", "example.com:12345"]
			}
		}
		region: {
			common:        false
			description:   "The region to send data to."
			required:      false
			relevant_when: "endpoint is not set"
			type: string: {
				default: null
				enum: {
					us:   "United States"
					eu:   "Europe"
					asia: "Asia"
				}
			}
		}
		customer_id: {
			description: "The Unique identifier (UUID) corresponding to the Chronicle instance."
			required:    true
			type: string: {
				examples: ["c8c65bfa-5f2c-42d4-9189-64bb7b939f2c"]
			}
		}
		log_type: {
			description: "Identifies the log entry. This must be one of the supported log types, otherwise Chronicle will reject the entry with an error."
			required:    true
			type: string: {
				examples: ["WINDOWS_DNS", "{{ log_type }}"]
				syntax: "template"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
	}

	telemetry: metrics: {
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_discarded_total:           components.sources.internal_metrics.output.metrics.events_discarded_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
