package metadata

components: transforms: throttle: {
	title: "Throttle"

	description: """
		Rate limits one or more log streams to limit load on downstream services, or to enforce usage quotas on users.
		"""

	classes: {
		development:   "stable"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		filter: {}
	}

	support: {
		requirements: []
		warnings: []
	}

	configuration: generated.components.transforms.throttle.configuration & {
		threshold: type: uint: examples: [100]
		window_secs: type: float: examples: [1.0]
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	output: {
		logs: "": {
			description: "The modified input `log` event."
		}
	}

	telemetry: metrics: {
		events_discarded_total: components.sources.internal_metrics.output.metrics.events_discarded_total
	}

	examples: [
		{
			title: "Rate limiting"
			input: [
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "First message"
						host:      "host-1.hostname.com"
					}
				},
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "Second message"
						host:      "host-1.hostname.com"
					}
				},
			]

			configuration: {
				threshold:   1
				window_secs: 60
			}

			output: [
				{
					log: {
						timestamp: "2020-10-07T12:33:21.223543Z"
						message:   "First message"
						host:      "host-1.hostname.com"
					}
				},
			]
		},
	]

	how_it_works: {
		rate_limiting: {
			title: "Rate Limiting"
			body:  """
				The `throttle` transform uses the [Generic Cell Rate Algorithm](\(urls.gcra)) to limit
				the rate of each bucket independently. Each bucket starts full and replenishes continuously.
				"""
			sub_sections: [
				{
					title: "Buckets"
					body: """
						The `throttle` transform buckets events into rate limiters based on the provided `key_field`, or a
						single bucket if not provided. Each bucket is rate limited separately.
						"""
				},
				{
					title: "Quotas"
					body: """
						With a fixed threshold, a full bucket admits a burst of `threshold` events. It then regains
						capacity for one event every `window_secs / threshold` seconds. For example, a threshold of
						10 over a 60-second window allows 10 events immediately, then one more every 6 seconds.
						"""
				},
				{
					title: "Dynamic thresholds"
					body: """
						An upstream transform can set a service's limit on each log. In this example, the
						`api` service gets 200 events per second. Each service without a limit gets the
						configured fallback of 20 events per second.

						```yaml
						transforms:
						  enriched_logs:
						    type: remap
						    inputs: [logs]
						    source: |
						      if .service == "api" {
						        %rate_limit = 200
						      }

						  service_throttle:
						    type: throttle
						    inputs: [enriched_logs]
						    key_field: "{{ service }}"
						    threshold: 20
						    threshold_field: "%rate_limit"
						    window_secs: 1
						```

						Each admitted event uses a fraction of its bucket equal to one divided by that event's
						threshold. For example, an event with a threshold of 2 uses half a full bucket. Two
						events with a threshold of 4 can then pass immediately in the same bucket.
						"""
				},
				{
					title: "Rate Limited Events"
					body: """
						When a bucket has too little capacity for an event, Vector drops the event and increments
						`component_discarded_events_total`. Enable `internal_metrics.emit_events_discarded_per_key`
						to also track drops by bucket in `events_discarded_total` with the `key` tag.
						"""
				},
			]
		}
	}
}
