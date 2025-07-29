package metadata

components: transforms: window: {
	title: "Window"

	description: """
		A variant of ring buffer or backtrace logging implemented as a sliding window. Keeps events in a buffer until
		the `flush_when` condition is matched. When the buffer is full, the oldest events are dropped.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		filter: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.transforms.window.configuration

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

	examples: [
		{
			title: "Flush recent events when an error happens"
			input: [
				{log: {message: "A01", level: "info"}},
				{log: {message: "A02", level: "debug"}},
				{log: {message: "A03", level: "info"}},
				{log: {message: "A04", level: "debug"}},
				{log: {message: "A05", level: "error"}},
				{log: {message: "A06", level: "debug"}},
				{log: {message: "A07", level: "warning"}},
				{log: {message: "A08", level: "info"}},
				{log: {message: "A09", level: "debug"}},
				{log: {message: "A10", level: "info"}},
			]

			configuration: {
				flush_when:        #".level == "error""#
				num_events_before: 2
				num_events_after:  2
			}

			output: [
				{log: {message: "A03", level: "info"}},
				{log: {message: "A04", level: "debug"}},
				{log: {message: "A05", level: "error"}},
				{log: {message: "A06", level: "debug"}},
				{log: {message: "A07", level: "warning"}},
			]
		},

		{
			title: "Pass events through without preserving the order"
			input: [
				{log: {message: "A01", level: "info"}},
				{log: {message: "A02", level: "debug"}},
				{log: {message: "A03", level: "info"}},
				{log: {message: "A04", level: "debug"}},
				{log: {message: "A05", level: "error"}},
				{log: {message: "A06", level: "debug"}},
				{log: {message: "A07", level: "warning"}},
				{log: {message: "A08", level: "info"}},
				{log: {message: "A09", level: "debug"}},
				{log: {message: "A10", level: "info"}},
			]

			configuration: {
				flush_when:        #".level == "error""#
				forward_when:      #".level == "info""#
				num_events_before: 2
				num_events_after:  2
			}

			output: [
				{log: {message: "A01", level: "info"}},
				{log: {message: "A03", level: "info"}},
				{log: {message: "A02", level: "debug"}},
				{log: {message: "A04", level: "debug"}},
				{log: {message: "A05", level: "error"}},
				{log: {message: "A06", level: "debug"}},
				{log: {message: "A07", level: "warning"}},
				{log: {message: "A08", level: "info"}},
				{log: {message: "A10", level: "info"}},
			]
		},
	]

	how_it_works: {
		advantages: {
			title: "Advantages of Use"
			body: """
				A common way to reduce log volume from a verbose system is to filter out less important messages, and only
				ingest errors and warnings. However, an error message by itself may not be sufficient to determine the
				cause, as surrounding events often contain important context information leading to the failure.

				The `window` transform allows for reduction of log volume by filtering out logs
				when the system is healthy, but preserving detailed logs when they are most relevant.
				"""
		}

		sliding_window: {
			title: "Sliding Window"
			body: """
				As the stream of events passes through the transform, it is observed though a "window" that spans between
				`num_events_before` and `num_events_after` relative to an event matched by the `flush_when` condition.  When the
				condition is matched, the whole window is flushed to the output. This is also known as backtrace logging or
				ring buffer logging.

				{{< svg "img/sliding-window.svg" >}}

				Past events are stored in a memory buffer with the capacity of `num_events_before`. When the buffer is full,
				the oldest events are dropped to make space for new ones. The buffer is not persistent, so in case of a hard
				system crash, all the buffered events will be lost.

				Future events are counted from the event matched by the `flush_when` condition until `num_events_after` number
				of events is reached.

				If the `flush_when` condition is matched before the buffer fills up, it will be flushed again. If the flush
				condition is triggered often enough (for example, the system is constantly logging errors), the transform may
				always be in the flushing state, meaning that no events will be filtered. Therefore it works best for conditions
				that are relatively uncommon.
				"""
		}
	}

	telemetry: metrics: {
		stale_events_flushed_total: components.sources.internal_metrics.output.metrics.stale_events_flushed_total
	}
}
