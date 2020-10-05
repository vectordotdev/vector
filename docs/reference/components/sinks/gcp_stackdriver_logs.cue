package metadata

components: sinks: gcp_stackdriver_logs: {
  title: "#{component.title}"
  short_description: "Batches log events to [Google Cloud Platform's Stackdriver Logging service][urls.gcp_stackdriver_logging] via the [REST Interface][urls.gcp_stackdriver_logging_rest]."
  long_description: "[Stackdriver][urls.gcp_stackdriver] is Google Cloud's embedded observability suite designed to monitor, troubleshoot, and improve cloud infrastructure, software and application performance. Stackdriver enables you to efficiently build and run workloads, keeping applications available and performing well."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_bytes: 5242880,
      max_events: null,
      timeout_secs: 1
    }
    buffer: enabled: true
    checkpoint: enabled: false
    compression: enabled: false
    encoding: {
      enabled: true
      default: null
      ndjson: null
      text: null
    }
    healthcheck: enabled: true
    multiline: enabled: false
    request: {
      enabled: true
      common: false,
      in_flight_limit: 5,
      rate_limit_duration_secs: 1,
      rate_limit_num: 1000,
      retry_initial_backoff_secs: 1,
      retry_max_duration_secs: 10,
      timeout_secs: 60
    }
    tls: {
      enabled: true
      can_enable: false
      can_verify_certificate: true
      can_verify_hostname: true
      enabled_default: false
    }
  }

  classes: {
    commonly_used: true
    function: "transmit"
    service_providers: ["GCP"]
  }

  statuses: {
    delivery: "at_least_once"
    development: "beta"
  }

  support: {
      input_types: ["log"]

    platforms: {
      "aarch64-unknown-linux-gnu": true
      "aarch64-unknown-linux-musl": true
      "x86_64-apple-darwin": true
      "x86_64-pc-windows-msv": true
      "x86_64-unknown-linux-gnu": true
      "x86_64-unknown-linux-musl": true
    }

    requirements: []
    warnings: []
  }

  configuration: {
    billing_account_id: {
      common: false
      description: "The billing account ID to which to publish logs.\n\nExactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["012345-6789AB-CDEF01"]
      }
    }
    credentials_path: {
      common: true
      description: "The filename for a Google Cloud service account credentials JSON file used to authenticate access to the Stackdriver Logging API. If this is unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename.\n\nIf no filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is running on. If Vector is not running on a GCE instance, you must define a credentials file as above."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["/path/to/credentials.json"]
      }
    }
    folder_id: {
      common: false
      description: "The folder ID to which to publish logs.\nSee the [Google Cloud Platform folder documentation][urls.gcp_folders] for more details.\n\nExactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["My Folder"]
      }
    }
    log_id: {
      description: "The log ID to which to publish logs. This is a name you create to identify this log stream."
      required: true
      warnings: []
      type: string: {
        examples: ["vector-logs"]
      }
    }
    organization_id: {
      common: false
      description: "The organization ID to which to publish logs. This would be the identifier assigned to your organization on Google Cloud Platform.\n\nExactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["622418129737"]
      }
    }
    project_id: {
      description: "The project ID to which to publish logs. See the [Google Cloud Platform project management documentation][urls.gcp_projects] for more details.\n\nExactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set."
      required: true
      warnings: []
      type: string: {
        examples: ["vector-123456"]
      }
    }
    resource: {
      common: false
      description: "Options for describing the logging resource."
      required: false
      warnings: []
      type: object: {
        examples: []
        options: {
          type: {
            description: "The monitored resource type. For example, the type of a Compute Engine VM instance is gce_instance.\n\nSee the [Google Cloud Platform monitored resource documentation][urls.gcp_resources] for more details."
            required: true
            warnings: []
            type: string: {
              examples: ["global","gce_instance"]
            }
          }
          "`[label]`": {
            common: false
            description: "Values for all of the labels listed in the associated monitored resource descriptor.\n\nFor example, Compute Engine VM instances use the labels `projectId`, `instanceId`, and `zone`."
            required: false
            warnings: []
            type: string: {
              default: null
              examples: [{"projectId":"vector-123456"},{"zone":"Twilight"}]
            }
          }
        }
      }
    }
    severity_key: {
      common: false
      description: "The field of the log event from which to take the outgoing log's `severity` field. The named field is removed from the log event if present, and must be either an integer between 0 and 800 or a string containing one of the [severity level names][urls.gcp_stackdriver_severity] (case is ignored) or a common prefix such as `err`. This could be added by an [`add_fields` transform][docs.transforms.add_fields] or extracted from a field from the source.\n\nIf no severity key is specified, the severity of outgoing records will be set to 0 (`DEFAULT`).\n\nSee the [GCP Stackdriver Logging LogSeverity description][urls.gcp_stackdriver_severity] for more details on the value of the `severity` field."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["severity"]
      }
    }
  }
}

