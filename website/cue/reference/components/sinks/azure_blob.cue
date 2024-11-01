package metadata

components: sinks: azure_blob: {
	title: "Azure Blob Storage"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Azure"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       true
				max_bytes:    10_000_000
				timeout_secs: 300.0
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
					framing: true
					enum: ["json", "text"]
				}
			}
			request: {
				enabled:        true
				rate_limit_num: 250
				headers:        false
			}
			tls: enabled: false
			to: {
				service: services.azure_blob

				interface: {
					socket: {
						api: {
							title: "Azure Blob Service REST API"
							url:   urls.azure_blob_endpoints
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

	configuration: base.components.sinks.azure_blob.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		object_naming: {
			title: "Object naming"
			body:  """
				By default, Vector names your blobs different based on whether or not the blobs are compressed.

				Here is the format without compression:

				```text
				<key_prefix><timestamp>-<uuidv4>.log
				```

				Here's an example blob name *without* compression:

				```text
				blob/2021-06-23/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log
				```

				And here is the format *with* compression:

				```text
				<key_prefix><timestamp>-<uuidv4>.log.gz
				```

				An example blob name with compression:

				```text
				blob/2021-06-23/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log.gz
				```

				Vector appends a [UUIDV4](\(urls.uuidv4)) token to ensure there are no name
				conflicts in the unlikely event that two Vector instances are writing data at the same
				time.

				You can control the resulting name via the [`blob_prefix`](#blob_prefix),
				[`blob_time_format`](#blob_time_format), and [`blob_append_uuid`](#blob_append_uuid) options.

				For example, to store objects at the root Azure storage folder, without a timestamp or UUID use
				these configuration options:

				```text
				blob_prefix = "{{ my_file_name }}"
				blob_time_format = ""
				blob_append_uuid = false
				```
				"""
		}
	}
}
