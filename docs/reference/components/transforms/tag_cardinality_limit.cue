package metadata

components: transforms: tag_cardinality_limit: {
  title: "Tag Cardinality Limit"
  short_description: "Accepts metric events and allows you to limit the cardinality of metric tags to prevent downstream disruption of metrics services."
  long_description: "Accepts metric events and allows you to limit the cardinality of metric tags to prevent downstream disruption of metrics services."

  classes: {
    commonly_used: true
    function: "filter"
  }

  features: {
    tls: enabled: false
  }

  statuses: {
    development: "beta"
  }

  support: {
    input_types: ["metric"]

    platforms: {
      "aarch64-unknown-linux-gnu": true
      "aarch64-unknown-linux-musl": true
      "x86_64-apple-darwin": true
      "x86_64-pc-windows-msv": true
      "x86_64-unknown-linux-gnu": true
      "x86_64-unknown-linux-musl": true
    }

    requirements: []
    warnings: []
  }

  configuration: {
    cache_size_per_tag: {
      common: false
      description: "The size of the cache in bytes to use to detect duplicate tags. The bigger the cache the less likely it is to have a 'false positive' or a case where we allow a new value for tag even after we have reached the configured limits."
      required: false
      warnings: []
      type: uint: {
        default: 5120000
        unit: "bytes"
      }
    }
    limit_exceeded_action: {
      common: true
      description: "Controls what should happen when a metric comes in with a tag that would exceed the configured limit on cardinality."
      required: false
      warnings: []
      type: string: {
        default: "drop_tag"
        enum: {
          drop_tag: "Remove tags that would exceed the configured limit from the incoming metric"
          drop_event: "Drop any metric events that contain tags that would exceed the configured limit"
        }
      }
    }
    mode: {
      description: "Controls what approach is used internally to keep track of previously seen tags and deterime when a tag on an incoming metric exceeds the limit."
      required: true
      warnings: []
      type: string: {
        enum: {
          exact: "Has higher memory requirements than `probabilistic`, but never falsely outputs metrics with new tags after the limit has been hit."
          probabilistic: "Has lower memory requirements than `exact`, but may occasionally allow metric events to pass through the transform even when they contain new tags that exceed the configured limit.  The rate at which this happens can be controlled by changing the value of `cache_size_per_tag`."
        }
      }
    }
    value_limit: {
      common: true
      description: "How many distinct values to accept for any given key."
      required: false
      warnings: []
      type: uint: {
        default: 500
        unit: null
      }
    }
  }
}
