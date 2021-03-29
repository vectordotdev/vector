package metadata

components: sinks: gcp_pubsub: {
	title: "GCP PubSub"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["GCP"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10485760
				max_events:   1000
				timeout_secs: 1
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			request: {
				enabled:                    true
				concurrency:                5
				rate_limit_duration_secs:   1
				rate_limit_num:             100
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               60
				headers:                    false
			}
			tls: {
				enabled:                true
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: services.gcp_pubsub

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
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		api_key: {
			common:      false
			description: "A [Google Cloud API key][urls.gcp_authentication_api_key] used to authenticate access the pubsub project and topic. Either this or `credentials_path` must be set."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["${GCP_API_KEY}", "ef8d5de700e7989468166c40fc8a0ccd"]
				syntax: "literal"
			}
		}
		credentials_path: {
			common:      true
			description: "The filename for a Google Cloud service account credentials JSON file used to authenticate access to the pubsub project and topic. If this is unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename.\n\nIf no filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is running on. If Vector is not running on a GCE instance, you must define a credentials file as above."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
				syntax: "literal"
			}
		}
		endpoint: {
			common:      false
			description: "The endpoint to which to send data."
			required:    false
			warnings: []
			type: string: {
				default: "https://pubsub.googleapis.com"
				examples: ["https://us-central1-pubsub.googleapis.com"]
				syntax: "literal"
			}
		}
		project: {
			description: "The project name to which to publish logs."
			required:    true
			warnings: []
			type: string: {
				examples: ["vector-123456"]
				syntax: "literal"
			}
		}
		topic: {
			description: "The topic within the project to which to publish logs."
			required:    true
			warnings: []
			type: string: {
				examples: ["this-is-a-topic"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "pubsub"

			policies: [
				{
					_action: "topics.get"
					required_for: ["healthcheck"]
				},
				{
					_action: "topics.publish"
					required_for: ["write"]
				},
			]
		},
	]
}
