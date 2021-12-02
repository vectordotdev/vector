remap: concepts: metrics: {
	title:       "Transforming metrics"
	description: """
		VRL enables you to transform [log events](\(urls.vector_log)) with very few restrictions but is much more
		restrictive when it comes to [metrics](\(urls.vector_metric)). These changes to metric events _are_ allowed:

		* You can add tags to the event via the `tags` field, which must be an
		  [object](\(urls.vrl_expressions)/#object). Here's an example:

		  ```coffee
		  .tags = {
			"env": "staging",
			"region": "nyc"
		  }
		  ```

		* You can change the namespace of the metric via the `namespace` field. Here's an example:

		  ```coffee
		  .namespace = "web"
		  ```

		* You can change the name of the metric via the `name` field. Here's an example:

		  ```coffee
		  if .name == "cpu" {
		    .name = "cpu_utilization"
		  }
		  ```

		These changes, however, are _not_ allowed:

		* The `timestamp` field can't be updated, which means that you can't, for example, reformat the timestamp.
		* The field holding the metric value—`counter`, `gauge`, `histogram`, etc.—can't be updated.

		If your VRL program makes disallowed changes, Vector ignores those changes without throwing an error.
		"""
}
