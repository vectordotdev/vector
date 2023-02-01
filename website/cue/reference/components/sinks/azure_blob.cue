package metadata

components: sinks: azure_blob: {
	title: "Azure Blob Storage"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Azure"]
		stateful: false
	}

	features: {
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

	configuration: {
		connection_string: {
			description: "The Azure Blob Storage Account connection string. Only authentication with access key supported. This or storage_account has to be provided."
			required:    false
			common:      true
			type: string: {
				default: ""
				examples: ["DefaultEndpointsProtocol=https;AccountName=mylogstorage;AccountKey=storageaccountkeybase64encoded;EndpointSuffix=core.windows.net"]
			}
		}
		storage_account: {
			description: "The Azure Blob Storage Account name. Credentials are read in this order: [EnvironmentCredential](https://docs.rs/azure_identity/latest/azure_identity/struct.DefaultAzureCredential.html), ManagedIdentityCredential, AzureCliCredential. This or connection_string has to be provided."
			required:    false
			common:      true
			type: string: {
				default: ""
				examples: ["mylogstorage"]
			}
		}
		endpoint: {
			description: "The Azure Blob Endpoint URL. This is used to override the default that is used when passing in the storage_account. Ignored if connection_string is used."
			required:    false
			common:      false
			type: string: {
				default: ""
				examples: ["https://test.blob.core.usgovcloudapi.net/", "https://test.blob.core.windows.net/"]
			}
		}
		container_name: {
			description: "The Azure Blob Storage Account container name."
			required:    true
			type: string: {
				examples: ["my-logs"]
			}
		}
		blob_prefix: {
			category:    "File Naming"
			common:      true
			description: "A prefix to apply to all object key names. This should be used to partition your objects, and it's important to end this value with a `/` if you want this to be the root azure storage \"folder\"."
			required:    false
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
			type: bool: default: true
		}
		blob_time_format: {
			category:    "File Naming"
			common:      false
			description: "The format of the resulting object file name. [`strftime` specifiers](\(urls.strptime_specifiers)) are supported."
			required:    false
			type: string: {
				default: "%s"
				syntax:  "strftime"
			}
		}
	}

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
