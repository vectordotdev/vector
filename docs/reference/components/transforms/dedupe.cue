package metadata

components: transforms: dedupe: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to prevent duplicate Events from being outputted by using an LRU cache."
  description: "Accepts log events and allows you to prevent duplicate Events from being outputted by using an LRU cache."

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
        type: object: {
          default: null
          examples: []
          options: {
            type: uint: {
              default: 5000
            }
          }
        }
    }
    fields: {
      common: true
      description: "Options controlling what fields to match against"
      required: true
        type: object: {
          examples: []
          options: {
            type: "[string]": {
              default: null
              examples: [["field1","parent.child_field"]]
            }
            type: "[string]": {
              default: ["timestamp","host","message"]
              examples: [["field1","parent.child_field"]]
            }
          }
        }
    }
  }
}