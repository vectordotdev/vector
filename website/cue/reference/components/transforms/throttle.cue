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
				With a fixed threshold, the `throttle` transform limits each bucket's average
				throughput to `threshold` per `window_secs`. It utilizes a [Generic Cell Rate Algorithm](\(urls.gcra)) to
				rate limit the event stream.
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
						Rate limiters use "cells" to determine if there is sufficient capacity for an event to successfully
						pass through a rate limiter. Each event passing through the transform consumes an available cell,
						if there is no available cell the event will be rate limited.

						With a fixed threshold, the maximum number of cells equals `threshold`, and cells replenish
						at a rate of `window_secs` divided by `threshold`. For example, a `window_secs` of 60 with a `threshold` of 10
						replenishes a cell every 6 seconds and allows a burst of up to 10 events.

						If an upstream transform sets `rate_limit` to 200 on each `api` log, this configuration allows
						200 events per second for `api`. Events with missing or invalid limits use the fallback of 20.
						Events for the same service share a bucket even if their thresholds change.

						```yaml
						transforms:
						  service_throttle:
						    type: throttle
						    inputs: [enriched_logs]
						    key_field: "{{ service }}"
						    threshold_field: rate_limit
						    threshold: 20
						    window_secs: 1
						```
						"""
				},
				{
					title: "Rate Limited Events"
					body: """
						The rate limiter checks whether the bucket has enough capacity for each event. Any event passed when the rate
						limiter is at capacity will be discarded and tracked by an `events_discarded_total` metric tagged
						by the bucket's `key`.
						"""
				},
			]
		}
	}
}
