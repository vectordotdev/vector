package metadata

components: sinks: webhdfs: {
	title: "WebHDFS"
	how_it_works: {
		retry_policy: {
			body: """
				Vector retries failed writes when OpenDAL classifies the error as temporary.
				Errors classified as permanent are not retried. Requests that exceed
				`request.timeout_secs` can also be retried.

				Retries reuse the same destination path and encoded batch. Configure
				the number of retries with `request.retry_attempts`, and configure
				the delay between retries with `request.retry_initial_backoff_secs`
				and `request.retry_max_duration_secs`.
				"""
		}
	}

	classes: {
		delivery: "at_least_once"

		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip", "zstd"]
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
				enabled:                 true
				headers:                 false
				retry_max_duration_secs: 30
			}
			tls: enabled: false
		}
	}

	support: {
		requirements: []
		warnings: []
	}

	configuration: generated.components.sinks.webhdfs.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}
}
