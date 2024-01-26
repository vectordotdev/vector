package metadata

base: components: transforms: log_to_metric: configuration: {
	all_metrics: {
		description: """
			Setting this flag changes the behavior of this transformation.<br />
			<p>Notably the `metrics` field will be ignored.</p>
			<p>All incoming events will be processed and if possible they will be converted to log events.
			Otherwise, only items specified in the 'metrics' field will be processed.</p>
			<pre class="chroma"><code class="language-toml" data-lang="toml">use serde_json::json;
			let json_event = json!({
			    "counter": {
			        "value": 10.0
			    },
			    "kind": "incremental",
			    "name": "test.transform.counter",
			    "tags": {
			        "env": "test_env",
			        "host": "localhost"
			    }
			});
			</code></pre>

			This is an example JSON representation of a counter with the following properties:

			- `counter`: An object with a single property `value` representing the counter value, in this case, `10.0`).
			- `kind`: A string indicating the kind of counter, in this case, "incremental".
			- `name`: A string representing the name of the counter, here set to "test.transform.counter".
			- `tags`: An object containing additional tags such as "env" and "host".

			Objects that can be processed include counter, histogram, gauge, set and summary.
			"""
		required: false
		type: bool: {}
	}
	metrics: {
		description: "A list of metrics to generate."
		required:    true
		type: array: items: type: object: options: {
			field: {
				description: "Name of the field in the event to generate the metric."
				required:    true
				type: string: syntax: "template"
			}
			increment_by_value: {
				description:   "Increments the counter by the value in `field`, instead of only by `1`."
				relevant_when: "type = \"counter\""
				required:      false
				type: bool: default: false
			}
			kind: {
				description: """
					Metric kind.

					Metrics can be either absolute of incremental. Absolute metrics represent a sort of "last write wins" scenario,
					where the latest absolute value seen is meant to be the actual metric value.  In contrast, and perhaps intuitively,
					incremental metrics are meant to be additive, such that we don't know what total value of the metric is, but we know
					that we'll be adding or subtracting the given value from it.

					Generally speaking, most metrics storage systems deal with incremental updates. A notable exception is Prometheus,
					which deals with, and expects, absolute values from clients.
					"""
				relevant_when: "type = \"counter\""
				required:      false
				type: string: {
					default: "incremental"
					enum: {
						absolute:    "Absolute metric."
						incremental: "Incremental metric."
					}
				}
			}
			name: {
				description: """
					Overrides the name of the counter.

					If not specified, `field` is used as the name of the metric.
					"""
				required: false
				type: string: syntax: "template"
			}
			namespace: {
				description: "Sets the namespace for the metric."
				required:    false
				type: string: syntax: "template"
			}
			tags: {
				description: "Tags to apply to the metric."
				required:    false
				type: object: options: "*": {
					description: "A metric tag."
					required:    true
					type: string: syntax: "template"
				}
			}
			type: {
				description: "The type of metric to create."
				required:    true
				type: string: enum: {
					counter:   "A counter."
					gauge:     "A gauge."
					histogram: "A histogram."
					set:       "A set."
					summary:   "A summary."
				}
			}
		}
	}
}
