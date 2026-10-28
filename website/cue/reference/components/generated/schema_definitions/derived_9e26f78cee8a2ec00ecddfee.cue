package metadata

_schemaDefinitions: "derived::9e26f78cee8a2ec00ecddfee": object: options: "*": {
	description: """
		Per-tag cardinality configuration.

		Specify `mode` to control how this tag is handled:

		Example:
		```yaml
		per_tag_limits:
		  environment:
		    mode: limit_override  # track with a per-tag cap
		    value_limit: 3
		  high_cardinality_tag:
		    mode: limit_override
		    value_limit: 1000
		    cache_size_per_key: 102400  # larger bloom filter for this tag
		  trace_id:
		    mode: excluded        # opt out of tracking entirely
		```
		"""
	required: true
	type:     _schemaDefinitions["vector::transforms::tag_cardinality_limit::config::PerTagConfig"]
}
