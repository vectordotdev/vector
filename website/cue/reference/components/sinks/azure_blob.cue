package metadata

components: sinks: azure_blob: {
	title: "Azure Blob Storage"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Azure"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       true
				max_bytes:    10485760
				timeout_secs: 300
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
					batched: true
					enum: ["ndjson", "text"]
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
		connection_string: {
			description: "The Azure Blob Storage Account connection string. Only authentication with access key supported."
			required:    true
			warnings: []
			type: string: {
				examples: ["DefaultEndpointsProtocol=https;AccountName=mylogstorage;AccountKey=storageaccountkeybase64encoded;EndpointSuffix=core.windows.net"]
				syntax: "literal"
			}
		}
		container_name: {
			description: "The Azure Blob Storage Account container name."
			required:    true
			warnings: []
			type: string: {
				examples: ["my-logs"]
				syntax: "literal"
			}
		}
		blob_prefix: {
			category:    "File Naming"
			common:      true
			description: "A prefix to apply to all object key names. This should be used to partition your objects, and it's important to end this value with a `/` if you want this to be the root azure storage \"folder\"."
			required:    false
			warnings: []
			type: string: {
				default: "blob/%F/"
				examples: ["date/%F/", "date/%F/hour/%H/", "year=%Y/month=%m/day=%d/", "kubernetes/{{ metadata.cluster }}/{{ metadata.application_name }}/"]
				syntax: "template"
			}
		}
		blob_append_uuid: {
			category:    "File Naming"
			common:      false
			description: "Whether or not to append a UUID v4 token to the end of the file. This ensures there are no name collisions high volume use cases."
			required:    false
			warnings: []
			type: bool: default: true
		}
		blob_time_format: {
			category:    "File Naming"
			common:      false
			description: "The format of the resulting object file name. [`strftime` specifiers](\(urls.strptime_specifiers)) are supported."
			required:    false
			warnings: []
			type: string: {
				default: "%s"
				syntax:  "strftime"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
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
				"""
		}
	}

	telemetry: metrics: {
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_discarded_total:           components.sources.internal_metrics.output.metrics.events_discarded_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
		http_error_response_total:        components.sources.internal_metrics.output.metrics.http_error_response_total
		http_request_errors_total:        components.sources.internal_metrics.output.metrics.http_request_errors_total
		processed_bytes_total:            components.sources.internal_metrics.output.metrics.processed_bytes_total
	}
}
