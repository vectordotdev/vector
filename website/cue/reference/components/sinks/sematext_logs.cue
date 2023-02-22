package metadata

components: sinks: sematext_logs: {
	title: "Sematext Logs"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Sematext"]
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
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
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: enabled: false
			to: sinks._sematext.features.send.to
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.sematext_logs.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body:  """
				1. Register for a free account at [Sematext.com](\(urls.sematext_registration))

				2. [Create a Logs App](\(urls.sematext_create_logs_app)) to get a Logs Token
				for [Sematext Logs](\(urls.sematext_logsense))
				"""
		}
	}

	telemetry: components.sinks.elasticsearch.telemetry
}
