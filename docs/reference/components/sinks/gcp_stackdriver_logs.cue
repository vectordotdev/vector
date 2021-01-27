package metadata

components: sinks: gcp_stackdriver_logs: {
	title: "GCP Operations (formerly Stackdrive) Logs"

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
				max_bytes:    5242880
				max_events:   null
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
				rate_limit_num:             1000
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
				service: services.gcp_operations_logs

				interface: {
					socket: {
						api: {
							title: "REST Interface"
							url:   urls.gcp_stackdriver_logging_rest
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
		billing_account_id: {
			common:      false
			description: "The billing account ID to which to publish logs.\n\nExactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["012345-6789AB-CDEF01"]
				syntax: "literal"
			}
		}
		credentials_path: {
			common:      true
			description: "The filename for a Google Cloud service account credentials JSON file used to authenticate access to the Stackdriver Logging API. If this is unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename.\n\nIf no filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is running on. If Vector is not running on a GCE instance, you must define a credentials file as above."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
				syntax: "literal"
			}
		}
		folder_id: {
			common:      false
			description: "The folder ID to which to publish logs.\nSee the [Google Cloud Platform folder documentation][urls.gcp_folders] for more details.\n\nExactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["My Folder"]
				syntax: "literal"
			}
		}
		log_id: {
			description: "The log ID to which to publish logs. This is a name you create to identify this log stream."
			required:    true
			warnings: []
			type: string: {
				examples: ["vector-logs"]
				syntax: "literal"
			}
		}
		organization_id: {
			common:      false
			description: "The organization ID to which to publish logs. This would be the identifier assigned to your organization on Google Cloud Platform.\n\nExactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["622418129737"]
				syntax: "literal"
			}
		}
		project_id: {
			description: "The project ID to which to publish logs. See the [Google Cloud Platform project management documentation][urls.gcp_projects] for more details.\n\nExactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set."
			required:    true
			warnings: []
			type: string: {
				examples: ["vector-123456"]
				syntax: "literal"
			}
		}
		resource: {
			description: "Options for describing the logging resource."
			required:    true
			warnings: []
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
						description: "The monitored resource type. For example, the type of a Compute Engine VM instance is gce_instance.\n\nSee the [Google Cloud Platform monitored resource documentation][urls.gcp_resources] for more details."
						required:    true
						warnings: []
						type: string: {
							examples: ["global", "gce_instance"]
							syntax: "literal"
						}
					}
					"*": {
						common:      false
						description: "Values for all of the labels listed in the associated monitored resource descriptor.\n\nFor example, Compute Engine VM instances use the labels `projectId`, `instanceId`, and `zone`."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["vector-123456", "Twilight"]
							syntax: "literal"
						}
					}
				}
			}
		}
		severity_key: {
			common:      false
			description: "The field of the log event from which to take the outgoing log's `severity` field. The named field is removed from the log event if present, and must be either an integer between 0 and 800 or a string containing one of the [severity level names][urls.gcp_stackdriver_severity] (case is ignored) or a common prefix such as `err`. This could be added by an [`add_fields` transform][docs.transforms.add_fields] or extracted from a field from the source.\n\nIf no severity key is specified, the severity of outgoing records will be set to 0 (`DEFAULT`).\n\nSee the [GCP Stackdriver Logging LogSeverity description][urls.gcp_stackdriver_severity] for more details on the value of the `severity` field."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["severity"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		severity_level_mapping: {
			title: "Severity Level Mapping"
			body: #"""
				If a `severity_key` is configured, outgoing log records will have their
				`severity` header field set from the named field in the Vector
				event. However, the [required values][urls.gcp_stackdriver_severity] for
				this field may be inconvenient to produce, typically requiring a custom
				mapping using an additional transform. To assist with this, this sink
				remaps certain commonly used words to the required numbers as in the
				following table. Note that only the prefix is compared, such that a
				value of `emergency` matches `emerg`, and the comparison ignores case.

				| Prefix | Value
				|:-------|:-----
				| emerg  | 800
				| fatal  | 800
				| alert  | 700
				| crit   | 600
				| err    | 500
				| warn   | 400
				| notice | 300
				| info   | 200
				| debug  | 100
				| trace  | 100
				"""#
		}
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "logging"

			policies: [
				{
					_action: "logEntries.create"
					required_for: ["healthcheck", "write"]
				},
			]
		},
	]
}
