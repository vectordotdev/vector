package metadata

components: transforms: merge: {
  title: "Merge"
  short_description: "Accepts log events and allows you to merge partial log events into a single event."
  long_description: "Accepts log events and allows you to merge partial log events into a single event."

  classes: {
    commonly_used: false
    function: "aggregate"
  }

  features: {
  }

  statuses: {
    development: "beta"
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
    fields: {
      common: true
      description: "Fields to merge. The values of these fields will be merged into the first partial event. Fields not specified here will be ignored. Merging process takes the first partial event and the base, then it merges in the fields from each successive partial event, until a non-partial event arrives. Finally, the non-partial event fields are merged in, producing the resulting merged event."
      required: false
      warnings: []
      type: "[string]": {
        default: ["message"]
        examples: [["message"],["message","parent.child"]]
      }
    }
    partial_event_marker_field: {
      common: true
      description: "The field that indicates that the event is partial. A consequent stream of partial events along with the first non-partial event will be merged together."
      required: false
      warnings: []
      type: string: {
        default: "_partial"
        examples: ["_partial","parent.child"]
      }
    }
    stream_discriminant_fields: {
      common: true
      description: "An ordered list of fields to distinguish streams by. Each stream has a separate partial event merging state. Should be used to prevent events from unrelated sources from mixing together, as this affects partial event processing."
      required: false
      warnings: []
      type: "[string]": {
        default: []
        examples: [["host"],["host","parent.child"]]
      }
    }
  }
}
