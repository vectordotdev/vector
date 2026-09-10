package metadata

generated: components: sources: prometheus_scrape: configuration: {
	auth: {
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::http::Auth>"]
	}
	endpoint_tag: {
		description: """
			The tag name added to each event representing the scraped instance's endpoint.

			The tag value is the endpoint of the scraped instance.
			"""
		required: false
		type: string: {}
	}
	endpoints: {
		description: "Endpoints to scrape metrics from."
		required:    true
		type: array: items: type: string: examples: ["http://localhost:9090/metrics"]
	}
	honor_labels: {
		description: """
			Controls how tag conflicts are handled if the scraped source has tags to be added.

			If `true`, the new tag is not added if the scraped metric has the tag already. If `false`, the conflicting tag
			is renamed by prepending `exported_` to the original name.

			This matches Prometheus’ `honor_labels` configuration.
			"""
		required: false
		type: bool: default: false
	}
	instance_tag: {
		description: """
			The tag name added to each event representing the scraped instance's `host:port`.

			The tag value is the host and port of the scraped instance.
			"""
		required: false
		type: string: {}
	}
	query: {
		description: """
			Custom parameters for the scrape request query string.

			One or more values for the same parameter key can be provided. The parameters provided in this option are
			appended to any parameters manually provided in the `endpoints` option. This option is especially useful when
			scraping the `/federate` endpoint.
			"""
		required: false
		type: object: {
			examples: [{
				"match[]": ["{job=\"somejob\"}", "{__name__=~\"job:.*\"}"]
			}]
			options: "*": {
				description: "A query string parameter."
				required:    true
				type:        _schemaDefinitions["vector::http::ParameterValue"]
			}
		}
	}
	scrape_delay: {
		description: """
			When scrapes happen, relative to the configured scrape interval.

			`none` scrapes as soon as the source starts and then every `scrape_interval_secs` exactly,
			which means the source raises load at one unvarying period, and sources that share an
			interval all scrape at the same instant.

			A delay in seconds, such as `30s`, holds the first scrape back by exactly that much and then
			keeps the same fixed cadence. Give each source a different value to stagger them by hand.

			`auto` chooses a position inside each interval independently. Under normal polling, the
			source starts one scrape round in each interval, but it does not remain at one fixed phase;
			intervals missed while the schedule is not polled are skipped rather than replayed. This
			reduces persistent alignment with other periodic work and with sources sharing the same
			interval. Two consecutive scrape starts can be anywhere from nearly zero to nearly two
			intervals apart, which can increase short-lived overlap and load compared with a fixed
			cadence. The scheduler does not enforce a minimum gap between starts or place an upper bound
			on in-flight scrapes. The positions come from a hash of the host name, the component ID and
			the scrape number rather than from a random number, so the sequence is reproducible relative
			to source start. Hash-derived positions do not guarantee distinct slots: individual scrapes
			can still coincide, and instances with the same host name and component ID use the same
			sequence.
			"""
		required: false
		type: string: {
			default: "none"
			examples: ["none", "auto", "30s"]
		}
	}
	scrape_interval_secs: {
		description: """
			The interval between scrapes. Requests are run concurrently so if a scrape takes longer
			than the interval a new scrape will be started. This can take extra resources, set the timeout
			to a value lower than the scrape interval to prevent this from happening.
			"""
		required: false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
	scrape_timeout_secs: {
		description: "The timeout for each scrape request."
		required:    false
		type: float: {
			default: 5.0
			unit:    "seconds"
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
