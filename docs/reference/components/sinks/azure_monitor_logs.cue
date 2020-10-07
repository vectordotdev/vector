package metadata

components: sinks: azure_monitor_logs: {
	title:             "Azure Monitor Logs"
	short_description: "Batches log events to [Azure Monitor's][urls.azure_monitor] logs via the [REST endpoint][urls.azure_monitor_logs_endpoints]."
	long_description:  "[Azure Monitor][urls.azure_monitor] is a service in Azure that provides performance and availability monitoring for applications and services in Azure, other cloud environments, or on-premises. Azure Monitor collects data from multiple sources into a common data platform where it can be analyzed for trends and anomalies."

	classes: {
		commonly_used: false
		function:      "transmit"
		service_providers: ["Azure"]
	}

	features: {
		batch: {
			enabled:      true
			common:       false
			max_bytes:    30000000
			max_events:   null
			timeout_secs: 1
		}
		buffer: enabled:      true
		compression: enabled: false
		encoding: {
			enabled: true
			default: null
			json:    null
			ndjson:  null
			text:    null
		}
		healthcheck: enabled: true
		request: enabled:     false
		tls: {
			enabled:                true
			can_enable:             true
			can_verify_certificate: true
			can_verify_hostname:    true
			enabled_default:        true
		}
	}

	statuses: {
		delivery:    "at_least_once"
		development: "beta"
	}

	support: {
		input_types: ["log"]

		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
	}

	configuration: {
		azure_resource_id: {
			common:      true
			description: "[Resource ID](https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#request-headers) of the Azure resource the data should be associated with."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/otherResourceGroup/providers/Microsoft.Storage/storageAccounts/examplestorage", "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/examplegroup/providers/Microsoft.SQL/servers/serverName/databases/databaseName"]
			}
		}
		customer_id: {
			description: "The [unique identifier](https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#request-uri-parameters) for the Log Analytics workspace."
			required:    true
			warnings: []
			type: string: {
				examples: ["5ce893d9-2c32-4b6c-91a9-b0887c2de2d6", "97ce69d9-b4be-4241-8dbd-d265edcf06c4"]
			}
		}
		log_type: {
			description: "The [record type of the data that is being submitted](https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#request-headers). Can only contain letters, numbers, and underscore (_), and may not exceed 100 characters."
			required:    true
			warnings: []
			type: string: {
				examples: ["MyTableName", "MyRecordType"]
			}
		}
		shared_key: {
			description: "The [primary or the secondary key](https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#authorization) for the Log Analytics workspace."
			required:    true
			warnings: []
			type: string: {
				examples: ["${AZURE_MONITOR_SHARED_KEY_ENV_VAR}", "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="]
			}
		}
	}
}
