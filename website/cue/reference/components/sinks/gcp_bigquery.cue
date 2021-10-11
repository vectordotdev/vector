package metadata

components: sinks: gcp_bigquery: {
	title: "GCP BigQuery"

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
				max_bytes:    8192000
				timeout_secs: 1
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					batched: true
					enum: ["ndjson"]
				}
			}
			proxy: enabled: true
			request: {
				enabled:        true
				concurrency:    25
				rate_limit_num: 1000
				headers:        false
			}
			tls: {
				enabled:                true
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: services.gcp_bigquery

				interface: {
					socket: {
						api: {
							title: "GCP BigQuery Streaming API"
							url:   urls.gcp_bigquery_streaming_api
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
		project: {
			description: "The Google Cloud project name."
			required:    true
			warnings: []
			type: string: {
				examples: ["my-project"]
				syntax: "literal"
			}
		}
		dataset: {
			description: "The BigQuery dataset name."
			required:    true
			warnings: []
			type: string: {
				examples: ["my-dataset"]
				syntax: "literal"
			}
		}
		table: {
			description: "The BigQuery table name."
			required:    true
			warnings: []
			type: string: {
				examples: ["my-table"]
				syntax: "literal"
			}
		}
		include_insert_id: {
			category:    "Deduplication"
			common:      false
			description: "Whether or not to include a UUID v4 insert id to enable best-effort deduplication. See https://cloud.google.com/bigquery/streaming-data-into-bigquery#dataconsistency"
			required:    false
			warnings: []
			type: bool: default: false
		}
		ignore_unknown_values: {
			common:      false
			description: "Indicates if BigQuery should allow extra values that are not represented in the table schema. If true, the extra values are ignored. If false, records with extra columns are treated as bad records, and an invalid error is returned in the response."
			required:    false
			warnings: []
			type: bool: default: false
		}
		skip_invalid_rows: {
			common:      false
			description: "Indicates if BigQuery should insert valid rows in a request that contains invalid rows."
			required:    false
			warnings: []
			type: bool: default: false
		}
		template_suffix: {
			common:      true
			description: "If included, BigQuery treats the targeted table as a base template, and creates a new table that shares the same schema as the targeted table and has a name that includes the specified suffix. See https://cloud.google.com/bigquery/streaming-data-into-bigquery#template-tables"
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
				syntax: "literal"
			}
		}
		credentials_path: {
			category:    "Auth"
			common:      true
			description: "The filename for a Google Cloud service account credentials JSON file used to authenticate access to the BigQuery API. If this is unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename.\n\nIf no filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is running on. If Vector is not running on a GCE instance, you must define a credentials file as above."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		bigquery_streaming_api: {
			title: "Streaming API"
			body:  """
            See https://cloud.google.com/bigquery/streaming-data-into-bigquery and https://cloud.google.com/bigquery/docs/reference/rest/v2/tabledata/insertAll.
            """
		}
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "bigquery"

			policies: [
				{
					_action: "bigquery.tables.updateData"
					required_for: ["operation"]
				},
				{
					_action: "bigquery.tables.get"
					required_for: ["healthcheck"]
				},
				{
					_action: "bigquery.datasets.get"
					required_for: ["healthcheck"]
				},
			]
		},
	]

	telemetry: metrics: {
		events_discarded_total:  components.sources.internal_metrics.output.metrics.events_discarded_total
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
