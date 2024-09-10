package metadata

components: sinks: greptimedb_logs: {
	title: "GreptimeDB Logs"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["GreptimeDB"]
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
				max_events:   20
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: false
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      false
			}
			to: {
				service: services.greptimedb

				interface: {
					socket: {
						api: {
							title: "GreptimeDB gRPC API"
							url:   urls.greptimedb_docs
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
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

	configuration: base.components.sinks.greptimedb_logs.configuration

	input: {
		logs: true
		metrics: {
			counter:      false
			distribution: false
			gauge:        false
			histogram:    false
			set:          false
			summary:      false
		}
		traces: false
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body:  """
				1. Start your own [GreptimeDB](\(urls.greptimedb)) or create an instance on [GreptimeCloud](\(urls.greptimecloud)).
				2. Configure gRPC endpoint(host:port) and optional dbname and authentication information.
				"""
		}
	}
}
