package metadata

_schemaDefinitions: "vector::sinks::elasticsearch::config::BulkConfig": object: options: {
	action: {
		description: """
			Action to use when making requests to the [Elasticsearch Bulk API][es_bulk].

			Only `index`, `create` and `update` actions are supported.

			[es_bulk]: https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html
			"""
		required: false
		type: string: {
			default: "index"
			examples: ["create", "{{ action }}"]
			syntax: "template"
		}
	}
	index: {
		description: "The name of the index to write events to."
		required:    false
		type: string: {
			default: "vector-%Y.%m.%d"
			examples: ["application-{{ application_id }}-%Y-%m-%d", "{{ index }}"]
			syntax: "template"
		}
	}
	template_fallback_index: {
		description: "The default index to write events to if the template in `bulk.index` cannot be resolved"
		required:    false
		type: string: examples: [
			"test-index"
		]
	}
	version: {
		description: "Version field value."
		required:    false
		type: string: {
			examples: ["{{ obj_version }}-%Y-%m-%d", "123"]
			syntax: "template"
		}
	}
	version_type: {
		description: """
			Version type.

			Possible values are `internal`, `external` or `external_gt` and `external_gte`.

			[es_index_versioning]: https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-index_.html#index-versioning
			"""
		required: false
		type: string: {
			default: "internal"
			enum: {
				external:     "The `external` or `external_gt` type."
				external_gte: "The `external_gte` type."
				internal:     "The `internal` type."
			}
			examples: ["internal", "external"]
		}
	}
}
