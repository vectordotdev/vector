package metadata

components: transforms: dedupe: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to prevent duplicate Events from being outputted by using an LRU cache."
  long_description: "Accepts log events and allows you to prevent duplicate Events from being outputted by using an LRU cache."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "filter"
  }

  statuses: {
    development: "stable"
  }

  support: {
      input_types: ["log"]

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
    cache: {
      common: false
      description: "Options controlling how we cache recent Events for future duplicate checking."
      required: false
      warnings: []
      type: object: {
        examples: []
        options: {
          num_events: {
            common: true
            description: "The number of recent Events to cache and compare new incoming Events against."
            required: false
            warnings: []
            type: uint: {
              default: 5000
              unit: null
            }
          }
        }
      }
    }
    fields: {
      description: "Options controlling what fields to match against"
      required: true
      warnings: []
      type: object: {
        examples: []
        options: {
          ignore: {
            common: false
            description: "The field names to ignore when deciding if an Event is a duplicate. Incompatible with the `fields.match` option."
            required: false
            warnings: []
            type: "[string]": {
              default: null
              examples: [["field1","parent.child_field"]]
            }
          }
          match: {
            common: true
            description: "The field names considered when deciding if an Event is a duplicate. This can\nalso be globally set via the [global `log_schema` options][docs.reference.global-options#log_schema].Incompatible with the `fields.ignore` option."
            required: false
            warnings: []
            type: "[string]": {
              default: ["timestamp","host","message"]
              examples: [["field1","parent.child_field"]]
            }
          }
        }
      }
    }
  }
}
