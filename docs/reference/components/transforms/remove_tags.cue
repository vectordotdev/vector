package metadata

components: transforms: remove_tags: {
  title: "#{component.title}"
  short_description: "Accepts metric events and allows you to remove one or more metric tags."
  description: "Accepts metric events and allows you to remove one or more metric tags."

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
    tags: {
      common: true
      description: "The tag names to drop."
      required: true
        type: "[string]": {
          examples: [["tag1","tag2"]]
        }
    }
  }
}