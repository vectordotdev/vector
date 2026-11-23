package metadata

_schemaDefinitions: "vector::sinks::util::adaptive_concurrency::AdaptiveConcurrencySettings": object: options: {
	decrease_ratio: {
		description: """
			The fraction of the current value to set the new concurrency limit when decreasing the limit.

			Valid values are greater than `0` and less than `1`. Smaller values cause the algorithm to scale back rapidly
			when latency increases.

			**Note**: The new limit is rounded down after applying this ratio.
			"""
		required: false
		type: float: default: 0.9
	}
	ewma_alpha: {
		description: """
			The weighting of new measurements compared to older measurements.

			Valid values are greater than `0` and less than `1`.

			ARC uses an exponentially weighted moving average (EWMA) of past RTT measurements as a reference to compare with
			the current RTT. Smaller values cause this reference to adjust more slowly, which may be useful if a service has
			unusually high response variability.
			"""
		required: false
		type: float: default: 0.4
	}
	initial_concurrency: {
		description: """
			The initial concurrency limit to use. If not specified, the initial limit is 1 (no concurrency).

			Datadog recommends setting this value to your service's average limit if you're seeing that it takes a
			long time to ramp up adaptive concurrency after a restart. You can find this value by looking at the
			`adaptive_concurrency_limit` metric.
			"""
		required: false
		type: uint: default: 1
	}
	max_concurrency_limit: {
		description: """
			The maximum concurrency limit.

			The adaptive request concurrency limit does not go above this bound. This is put in place as a safeguard.
			"""
		required: false
		type: uint: default: 200
	}
	rtt_deviation_scale: {
		description: """
			Scale of RTT deviations which are not considered anomalous.

			Valid values are greater than or equal to `0`, and reasonable values range from `1.0` to `3.0`.

			When calculating the past RTT average, a secondary “deviation” value is also computed that indicates how variable
			those values are. That deviation is used when comparing the past RTT average to the current measurements, so we
			can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
			an appropriate range. Larger values cause the algorithm to ignore larger increases in the RTT.
			"""
		required: false
		type: float: default: 2.5
	}
}
