package metadata

generated: components: transforms: redis: configuration: {
	cache_max_size: {
		description: """
			Maximum number of Redis lookup results to cache in memory.

			When set, Redis lookup results are cached to reduce round-trips to Redis.
			The cache uses an LRU (Least Recently Used) eviction policy.

			If not specified, caching is disabled and every lookup will query Redis.
			"""
		required: false
		type: uint: examples: [1000, 10000]
	}
	cache_ttl: {
		description: """
			Time-to-live (TTL) for cached Redis lookup results.

			When set, cached entries will expire after the specified duration.
			Expired entries are automatically refreshed from Redis on the next lookup
			instead of being evicted from the cache.

			If not specified, cached entries do not expire.
			"""
		required: false
		type: uint: {
			examples: [300000, 3600000]
			unit: "milliseconds"
		}
	}
	concurrency_limit: {
		description: """
			Maximum number of concurrent Redis lookups.

			This limits the number of Redis lookups that can be in-flight simultaneously.
			Higher values allow more parallelism but may increase Redis connection pressure.

			Defaults to 100 if not specified.
			"""
		required: false
		type: uint: examples: [50, 200]
	}
	connection_timeout: {
		description: """
			Timeout for establishing connection to Redis and verifying connectivity during startup.

			If Redis is unavailable or doesn't respond within this timeout, Vector will fail to start.
			This ensures Vector fails fast during startup rather than starting and failing later when events need to be enriched.

			Defaults to 5 seconds if not specified.
			"""
		required: false
		type: uint: {
			default: 5000
			examples: [5000, 10000]
			unit: "milliseconds"
		}
	}
	default_value: {
		description: """
			The default value to use if the Redis key is not found.

			If not specified, the field will not be added when the key is missing.
			"""
		required: false
		type: string: examples: ["default_value", ""]
	}
	key: {
		description: """
			The Redis key template to use for lookups.

			This template is evaluated for each event to determine the Redis key to look up.
			The template can use event fields using the `{{ field_name }}` syntax, for example: `user:{{ user_id }}`.
			"""
		required: true
		type: string: {
			examples: ["user:{{ user_id }}", "session:{{ session_id }}"]
			syntax: "template"
		}
	}
	output_field: {
		description: """
			The field path where the Redis lookup value will be stored.

			If the Redis key is not found, the field will not be added to the event.
			"""
		required: true
		type: string: examples: ["redis_data", "enrichment.user_data"]
	}
	url: {
		description: """
			The Redis URL to connect to.

			The URL must take the form of `protocol://server:port/db` where the `protocol` can either be `redis` or `rediss` for connections secured using TLS.
			"""
		required: true
		type: string: examples: ["redis://127.0.0.1:6379/0"]
	}
}
