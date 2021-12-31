package metadata

components: sinks: azure_monitor_logs: {
	title: "Azure Monitor Logs"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
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
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled:   true
			request: enabled: false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
			}
			to: {
				service: services.azure_monitor_logs

				interface: {
					socket: {
						api: {
							title: "Azure Monitor logs API"
							url:   urls.azure_monitor_logs_endpoints
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
		azure_resource_id: {
			common:      true
			description: "[Resource ID](https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#request-headers) of the Azure resource the data should be associated with."
			required:    false
			type: string: {
				default: null
				examples: ["/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/otherResourceGroup/providers/Microsoft.Storage/storageAccounts/examplestorage", "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/examplegroup/providers/Microsoft.SQL/servers/serverName/databases/databaseName"]
			}
		}
		customer_id: {
			description: "The [unique identifier](https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#request-uri-parameters) for the Log Analytics workspace."
			required:    true
			type: string: {
				examples: ["5ce893d9-2c32-4b6c-91a9-b0887c2de2d6", "97ce69d9-b4be-4241-8dbd-d265edcf06c4"]
			}
		}
		host: {
			common:      true
			description: "[Alternative host](https://docs.azure.cn/en-us/articles/guidance/developerdifferences#check-endpoints-in-azure) for dedicated Azure regions."
			required:    false
			type: string: {
				default: "ods.opinsights.azure.com"
				examples: ["ods.opinsights.azure.us", "ods.opinsights.azure.cn"]
			}
		}
		log_type: {
			description: "The [record type of the data that is being submitted](https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#request-headers). Can only contain letters, numbers, and underscore (_), and may not exceed 100 characters."
			required:    true
			type: string: {
				examples: ["MyTableName", "MyRecordType"]
			}
		}
		shared_key: {
			description: "The [primary or the secondary key](https://docs.microsoft.com/en-us/azure/azure-monitor/platform/data-collector-api#authorization) for the Log Analytics workspace."
			required:    true
			type: string: {
				examples: ["${AZURE_MONITOR_SHARED_KEY_ENV_VAR}", "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
	}
}
