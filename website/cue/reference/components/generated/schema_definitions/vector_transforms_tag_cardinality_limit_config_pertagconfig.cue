package metadata

_schemaDefinitions: "vector::transforms::tag_cardinality_limit::config::PerTagConfig": object: options: {
	cache_size_per_key: {
		description: """
			Override the bloom filter cache size for this specific tag key.
			Only valid in `probabilistic` mode; setting this in `exact` mode is a configuration error.
			Inherits from the enclosing config when unset.
			"""
		relevant_when: "mode = \"limit_override\""
		required:      false
		type: uint: {}
	}
	mode: {
		description: "Controls how this tag key is handled."
		required:    true
		type: string: enum: {
			excluded: """
				Opt this tag out of cardinality tracking entirely. All values pass through
				without being recorded or checked against any `value_limit`.
				"""
			limit_override: """
				Track this tag with a per-tag value limit. All other settings are inherited from
				the enclosing config.
				"""
		}
	}
	value_limit: {
		description:   "Maximum number of distinct values to accept for this tag key."
		relevant_when: "mode = \"limit_override\""
		required:      true
		type: uint: {}
	}
}
