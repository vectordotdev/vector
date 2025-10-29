package metadata

components: sinks: azure_logs_ingestion: {
	title: "Azure Logs Ingestion"

	description: """
		This sink uses the Azure Monitor Logs Ingestion API to send log events to a Log Analytics Workspace.

		The `azure_identity` crate is used for authentication, which supports the standard Azure authentication types
		(Workload Identity, Managed Identity, Azure CLI, Service Principal with Certificate or Secret, etc.) through
		environment variables.
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
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
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1.0
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
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.azure_logs_ingestion

				interface: {
					socket: {
						api: {
							title: "Azure Monitor Logs Ingestion API"
							url:   urls.azure_logs_ingestion_endpoints
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

	configuration: base.components.sinks.azure_logs_ingestion.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}
}
