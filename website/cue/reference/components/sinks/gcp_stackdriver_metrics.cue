package metadata

components: sinks: gcp_stackdriver_metrics: {
	title: "GCP Cloud Monitoring (formerly Stackdrive) Metrics"

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
		healthcheck: enabled: false
		send: {
			batch: {
				enabled:      true
				common:       false
				max_events:   1
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
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
				service: services.gcp_cloud_monitoring

				interface: {
					socket: {
						api: {
							title: "REST Interface"
							url:   urls.gcp_stackdriver_metrics_rest
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
			common:      true
			description: "The filename for a Google Cloud service account credentials JSON file used to authenticate access to the Stackdriver Logging API. If this is unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename.\n\nIf no filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is running on. If Vector is not running on a GCE instance, you must define a credentials file as above."
			required:    false
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
			}
		}
		project_id: {
			description: "The project ID to which to publish logs. See the [Google Cloud Platform project management documentation](\(urls.gcp_projects)) for more details.\n\nExactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set."
			required:    true
			type: string: {
				examples: ["vector-123456"]
			}
		}
		default_namespace: {
			common:      false
			description: "The namespace used if the metric we are going to send to GCP has no namespace."
			required:    false
			type: string: {
				examples: ["vector-123456"]
				default: "namespace"
			}
		}
		resource: {
			description: "Options for describing the logging resource."
			required:    true
			type: object: {
				examples: [
					{
						type:       "global"
						projectId:  "vector-123456"
						instanceId: "Twilight"
						zone:       "us-central1-a"
					},
				]
				options: {
					type: {
						description: "The monitored resource type. For example, the type of a Compute Engine VM instance is gce_instance.\n\nSee the [Google Cloud Platform monitored resource documentation](\(urls.gcp_resources)) for more details."
						required:    true
						type: string: {
							examples: ["global", "gce_instance"]
						}
					}
					"*": {
						common:      false
						description: "Values for all of the labels listed in the associated monitored resource descriptor.\n\nFor example, Compute Engine VM instances use the labels `projectId`, `instanceId`, and `zone`."
						required:    false
						type: string: {
							default: null
							examples: ["vector-123456", "Twilight"]
						}
					}
				}
			}
		}
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: false
			gauge:        true
			histogram:    false
			set:          false
			summary:      false
		}
		traces: false
	}

	how_it_works: {
		duplicate_tags_names: {
			title: "Duplicate tag names"
			body: """
					Multiple tags with the same name cannot be sent to GCP. Vector will only send
					the last value for each tag name.
				"""
		}
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "monitoring"

			policies: [
				{
					_action: "timeSeries.create"
					required_for: ["healthcheck", "operation"]
				},
			]
		},
	]

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
	}
}
