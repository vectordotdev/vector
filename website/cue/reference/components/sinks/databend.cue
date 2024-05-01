package metadata

components: sinks: databend: {
	title: "Databend"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Databend"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "gzip"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "csv"]
				}
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: {
				service: services.databend

				interface: {
					socket: {
						api: {
							title: "Databend HTTP REST API"
							url:   urls.databend_rest
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: [
			"""
				[Databend](\(urls.databend)) version `>= 1.2.216` is required.
				""",
		]
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.databend.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		staged_sink: {
			title: "Staged Sink"
			body: """
				The `Databend` sink will do a 3-step batch sink by default:
				1. Get a presigned url for object storage before a batch by the query:
				    ```sql
				    PRESIGN UPLOAD @stage_name/stage_path;
				    ```
				    The `stage_name` default to user stage: `~`.
				    The `stage_path` generated from: `vector/{database}/{table}/{timestamp}-{random_suffix}`
				2. Format data into ndjson, and upload directly into object storage with the presigned url.
				3. Insert with the uploaded file with stage attachment in previous step.
				    ref: https://docs.databend.com/developer/apis/http#stage-attachment
				"""
		}
	}
}
