package metadata

components: transforms: add_tags: {
  title: "#{component.title}"
  short_description: "Accepts metric events and allows you to add one or more metric tags."
  description: "Accepts metric events and allows you to add one or more metric tags."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "schema"
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
    overwrite: {
      common: true
      description: "By default, fields will be overridden. Set this to `false` to avoid overwriting values.\n"
      required: false
        type: bool: default: true
    }
    tags: {
      common: true
      description: "A table of key/value pairs representing the tags to be added to the metric."
      required: false
        type: object: {
          default: null
          examples: []
          options: {
            type: string: {
              examples: [{"static_tag":"my value"},{"env_tag":"${ENV_VAR}"}]
            }
          }
        }
    }
  }
}